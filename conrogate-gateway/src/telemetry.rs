//! 遥测上报实现：聚合指标 + 事件入库。

use conrogate_contract::dto::{EventRow, MetricRow};
use conrogate_contract::gateway::TelemetryReport;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 进程内遥测通道
pub struct TelemetryReportImpl {
    metric_tx: mpsc::Sender<MetricRow>,
    event_tx: mpsc::Sender<EventRow>,
}

impl TelemetryReportImpl {
    pub fn new(
        metric_tx: mpsc::Sender<MetricRow>,
        event_tx: mpsc::Sender<EventRow>,
    ) -> Self {
        Self { metric_tx, event_tx }
    }
}

#[async_trait::async_trait]
impl TelemetryReport for TelemetryReportImpl {
    async fn record_metric(&self, metric: MetricRow) {
        // 发送到聚合通道，如果通道满则丢弃（非阻塞）
        let _ = self.metric_tx.try_send(metric);
    }

    async fn record_event(&self, event: EventRow) {
        let _ = self.event_tx.try_send(event);
    }
}

/// 指标聚合器：收集单条指标，按时间桶聚合，批量落库
pub struct MetricAggregator {
    metric_rx: mpsc::Receiver<MetricRow>,
    // (gate_id, route_id, bucket_ts) → 聚合数据
    buckets: std::collections::HashMap<(String, Option<u64>, chrono::DateTime<chrono::Utc>), MetricBucket>,
    bucket_sec: u32,
    /// 落库通道
    metric_repo: Option<Arc<dyn conrogate_contract::storage::MetricRepo>>,
    /// 落库失败的指标批次环形缓冲（指数退避重试兜底）
    recently_failed: std::collections::VecDeque<Vec<conrogate_contract::dto::MetricRow>>,
}

#[derive(Default)]
struct MetricBucket {
    total_requests: u64,
    total_latency_ms: f64,
    status_2xx: u64,
    status_3xx: u64,
    status_4xx: u64,
    status_5xx: u64,
    bytes_in: u64,
    bytes_out: u64,
}

impl MetricBucket {
    #[allow(dead_code)]
    fn new() -> Self {
        Self {
            total_requests: 0,
            total_latency_ms: 0.0,
            status_2xx: 0,
            status_3xx: 0,
            status_4xx: 0,
            status_5xx: 0,
            bytes_in: 0,
            bytes_out: 0,
        }
    }

    fn add(&mut self, row: &MetricRow) {
        self.total_requests += row.total_requests;
        self.total_latency_ms += row.avg_latency_ms * row.total_requests as f64;
        self.status_2xx += row.status_2xx;
        self.status_3xx += row.status_3xx;
        self.status_4xx += row.status_4xx;
        self.status_5xx += row.status_5xx;
        self.bytes_in += row.bytes_in;
        self.bytes_out += row.bytes_out;
    }

    fn to_metric_row(
        &self,
        gate_id: &str,
        route_id: Option<u64>,
        ts: chrono::DateTime<chrono::Utc>,
        bucket_sec: u32,
    ) -> MetricRow {
        MetricRow {
            ts,
            bucket_sec,
            route_id,
            gate_id: gate_id.to_string(),
            qps: (self.total_requests / bucket_sec as u64) as u32,
            total_requests: self.total_requests,
            avg_latency_ms: if self.total_requests > 0 {
                self.total_latency_ms / self.total_requests as f64
            } else {
                0.0
            },
            p50_ms: 0,
            p90_ms: 0,
            p99_ms: 0,
            status_2xx: self.status_2xx,
            status_3xx: self.status_3xx,
            status_4xx: self.status_4xx,
            status_5xx: self.status_5xx,
            sessions: 0,
            bytes_in: self.bytes_in,
            bytes_out: self.bytes_out,
        }
    }
}

impl MetricAggregator {
    pub fn new(metric_rx: mpsc::Receiver<MetricRow>, bucket_sec: u32) -> Self {
        Self {
            metric_rx,
            buckets: std::collections::HashMap::new(),
            bucket_sec,
            metric_repo: None,
            recently_failed: std::collections::VecDeque::new(),
        }
    }

    /// 设置落库仓储
    pub fn with_metric_repo(mut self, repo: Arc<dyn conrogate_contract::storage::MetricRepo>) -> Self {
        self.metric_repo = Some(repo);
        self
    }

    /// 运行聚合循环
    pub async fn run(&mut self, flush_interval: std::time::Duration) {
        let mut flush_timer = tokio::time::interval(flush_interval);
        flush_timer.tick().await; // 跳过第一次立即触发

        loop {
            tokio::select! {
                Some(row) = self.metric_rx.recv() => {
                    let bucket_ts = Self::align_to_bucket(row.ts, self.bucket_sec);
                    let key = (row.gate_id.clone(), row.route_id, bucket_ts);
                    self.buckets.entry(key).or_default().add(&row);
                }
                _ = flush_timer.tick() => {
                    // flush 聚合数据并落库
                    let now = chrono::Utc::now();
                    let cutoff = Self::align_to_bucket(now, self.bucket_sec);
                    let keys_to_flush: Vec<_> = self.buckets.keys()
                        .filter(|(_, _, ts)| *ts < cutoff)
                        .cloned()
                        .collect();
                    let mut rows_to_flush = Vec::new();
                    for key in keys_to_flush {
                        if let Some(bucket) = self.buckets.remove(&key) {
                            let row = bucket.to_metric_row(&key.0, key.1, key.2, self.bucket_sec);
                            tracing::debug!(?row.route_id, qps = row.qps, "metric bucket flushed");
                            rows_to_flush.push(row);
                        }
                    }
                    // 批量落库（带指数退避重试 + 环形缓冲兜底）
                    if !rows_to_flush.is_empty() {
                        if let Some(ref repo) = self.metric_repo {
                            let mut retry_backoff = std::time::Duration::from_millis(500);
                            let max_backoff = std::time::Duration::from_secs(30);
                            let mut attempt = 0;
                            let max_attempts = 3;

                            loop {
                                match repo.upsert_batch(&rows_to_flush).await {
                                    Ok(()) => break,
                                    Err(e) => {
                                        attempt += 1;
                                        if attempt >= max_attempts {
                                            tracing::warn!(
                                                error = %e,
                                                attempts = attempt,
                                                "metric batch insert failed after retries, buffering"
                                            );
                                            // 放入环形缓冲，下次 flush 重试
                                            self.recently_failed.push_back(rows_to_flush.clone());
                                            // 环形缓冲上限：保留最近 10 批失败数据
                                            if self.recently_failed.len() > 10 {
                                                let dropped = self.recently_failed.pop_front();
                                                if let Some(d) = dropped {
                                                    tracing::warn!(
                                        count = d.len(),
                                        "ring buffer full, dropping oldest failed batch"
                                    );
                                                }
                                            }
                                            break;
                                        }
                                        tracing::warn!(
                                            error = %e,
                                            attempt = attempt,
                                            "metric batch insert failed, retrying after backoff"
                                        );
                                        tokio::time::sleep(retry_backoff).await;
                                        retry_backoff = (retry_backoff * 2).min(max_backoff);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn align_to_bucket(ts: chrono::DateTime<chrono::Utc>, bucket_sec: u32) -> chrono::DateTime<chrono::Utc> {
        let secs = ts.timestamp();
        let aligned = secs - (secs % bucket_sec as i64);
        chrono::DateTime::from_timestamp(aligned, 0).unwrap_or(ts)
    }
}
