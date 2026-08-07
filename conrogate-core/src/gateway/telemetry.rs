//! 遥测上报实现：聚合指标 + 事件入库。

use crate::contract::dto::{EventRow, MetricRow};
use crate::contract::gateway::TelemetryReport;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 进程内遥测通道
pub struct TelemetryReportImpl {
    metric_tx: mpsc::Sender<MetricRow>,
    event_tx: mpsc::Sender<EventRow>,
}

impl TelemetryReportImpl {
    pub fn new(metric_tx: mpsc::Sender<MetricRow>, event_tx: mpsc::Sender<EventRow>) -> Self {
        Self {
            metric_tx,
            event_tx,
        }
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
    buckets: std::collections::HashMap<
        (String, Option<u64>, chrono::DateTime<chrono::Utc>),
        MetricBucket,
    >,
    bucket_sec: u32,
    /// 落库通道
    metric_repo: Option<Arc<dyn crate::contract::storage::MetricRepo>>,
    /// 落库失败的指标批次环形缓冲（指数退避重试兜底）
    recently_failed: std::collections::VecDeque<Vec<crate::contract::dto::MetricRow>>,
}

#[derive(Default)]
struct MetricBucket {
    total_requests: u64,
    total_latency_ms: f64,
    latency_hist: LatencyHistogram,
    status_2xx: u64,
    status_3xx: u64,
    status_4xx: u64,
    status_5xx: u64,
    bytes_in: u64,
    bytes_out: u64,
}

/// 延迟直方图：指数桶（固定内存），用于计算 p50/p90/p99。
///
/// 桶 k 覆盖 [2^k, 2^(k+1)) 毫秒（k=0 覆盖 [0,2)），
/// 百分位取累计数达到阈值时的桶上界，保证 SLO 风格的保守估计。
struct LatencyHistogram {
    bins: Vec<u64>,
}

impl LatencyHistogram {
    /// 覆盖 0 ~ 2^26 ms（≈18.6h），对网关超时量级足够
    const BIN_COUNT: usize = 26;

    fn new() -> Self {
        Self {
            bins: vec![0; Self::BIN_COUNT],
        }
    }

    fn bin_index(latency_ms: u32) -> usize {
        let idx = if latency_ms == 0 {
            0
        } else {
            latency_ms.ilog2() as usize
        };
        idx.min(Self::BIN_COUNT - 1)
    }

    fn add(&mut self, latency_ms: u32, count: u64) {
        self.bins[Self::bin_index(latency_ms)] += count;
    }

    fn total(&self) -> u64 {
        self.bins.iter().sum()
    }

    /// 取百分位（0 < p <= 1.0）：累计数首次达到 threshold 的桶上界；无样本返回 0
    fn percentile(&self, p: f64) -> u32 {
        let total = self.total();
        if total == 0 {
            return 0;
        }
        let threshold = ((total as f64) * p).ceil() as u64;
        let mut cum = 0u64;
        for (k, &count) in self.bins.iter().enumerate() {
            cum += count;
            if cum >= threshold {
                // 桶 k 上界：2^(k+1) ms（k=0 → 2）
                return 1u32 << (k + 1).min(30);
            }
        }
        0
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricBucket {
    fn add(&mut self, row: &MetricRow) {
        self.total_requests += row.total_requests;
        self.total_latency_ms += row.avg_latency_ms * row.total_requests as f64;
        // 原始样本 total_requests 恒为 1；按均值归桶近似（均值即单样本延迟）
        self.latency_hist
            .add(row.avg_latency_ms as u32, row.total_requests);
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
            p50_ms: self.latency_hist.percentile(0.5),
            p90_ms: self.latency_hist.percentile(0.9),
            p99_ms: self.latency_hist.percentile(0.99),
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
    pub fn with_metric_repo(
        mut self,
        repo: Arc<dyn crate::contract::storage::MetricRepo>,
    ) -> Self {
        self.metric_repo = Some(repo);
        self
    }

    /// 尝试落库（指数退避重试）；最终失败写入环形缓冲，待下次 flush 重试
    async fn flush_rows(&mut self, rows: Vec<MetricRow>) {
        if rows.is_empty() {
            return;
        }
        let Some(repo) = self.metric_repo.clone() else {
            return;
        };
        let mut retry_backoff = std::time::Duration::from_millis(500);
        let max_backoff = std::time::Duration::from_secs(30);
        let mut attempt = 0;
        let max_attempts = 3;

        loop {
            match repo.upsert_batch(&rows).await {
                Ok(()) => return,
                Err(e) => {
                    attempt += 1;
                    if attempt >= max_attempts {
                        tracing::warn!(
                            error = %e,
                            attempts = attempt,
                            "metric batch insert failed after retries, buffering"
                        );
                        // 放入环形缓冲，下次 flush 重试
                        self.recently_failed.push_back(rows.clone());
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
                        return;
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
                    // 先重试环形缓冲中的失败批次，再落库新批次
                    let buffered: Vec<Vec<MetricRow>> = self.recently_failed.drain(..).collect();
                    for batch in buffered {
                        self.flush_rows(batch).await;
                    }
                    self.flush_rows(rows_to_flush).await;
                }
            }
        }
    }

    fn align_to_bucket(
        ts: chrono::DateTime<chrono::Utc>,
        bucket_sec: u32,
    ) -> chrono::DateTime<chrono::Utc> {
        let secs = ts.timestamp();
        let aligned = secs - (secs % bucket_sec as i64);
        chrono::DateTime::from_timestamp(aligned, 0).unwrap_or(ts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_percentile_empty() {
        let hist = LatencyHistogram::new();
        assert_eq!(hist.percentile(0.5), 0);
    }

    #[test]
    fn histogram_zero_latency() {
        let mut hist = LatencyHistogram::new();
        hist.add(0, 1);
        assert_eq!(hist.percentile(0.5), 2);
        assert_eq!(hist.percentile(0.99), 2);
    }

    #[test]
    fn histogram_percentile_single_sample() {
        let mut hist = LatencyHistogram::new();
        hist.add(42, 1);
        assert_eq!(hist.percentile(0.5), 64, "42ms 落在 [32,64) 桶，取上界 64");
        assert_eq!(hist.percentile(0.99), 64);
    }

    #[test]
    fn histogram_percentile_monotonic() {
        let mut hist = LatencyHistogram::new();
        for ms in [1u32, 2, 4, 8, 16, 32, 64, 128] {
            hist.add(ms, 100);
        }
        let p50 = hist.percentile(0.5);
        let p90 = hist.percentile(0.9);
        let p99 = hist.percentile(0.99);
        assert!(p50 <= p90 && p90 <= p99, "p50={p50} p90={p90} p99={p99}");
        assert_eq!(p50, 16, "累计第 400 个样本落在 [8,16) 桶，取上界 16");
    }

    /// 聚合行落库的 p50/p90/p99 不再恒为 0，且能捕获慢请求尾部
    #[test]
    fn metric_bucket_to_row_has_percentiles() {
        let mut bucket = MetricBucket::default();
        let base = chrono::Utc::now();
        // 90 个低延迟 + 10 个慢请求：p99 应显著高于 p50
        for _ in 0..90 {
            bucket.add(&MetricRow::raw_sample(
                base,
                "gw-1".into(),
                Some(1),
                10.0,
                10,
                10,
                10,
                1,
                0,
                0,
                0,
                0,
                0,
                0,
            ));
        }
        for _ in 0..10 {
            bucket.add(&MetricRow::raw_sample(
                base,
                "gw-1".into(),
                Some(1),
                2000.0,
                2000,
                2000,
                2000,
                1,
                0,
                0,
                0,
                0,
                0,
                0,
            ));
        }

        let row = bucket.to_metric_row("gw-1", Some(1), base, 10);
        assert_eq!(row.total_requests, 100);
        assert_eq!(row.p50_ms, 16, "10ms 落在 [8,16) 桶，取上界 16");
        assert_eq!(row.p99_ms, 2048, "2000ms 落在 [1024,2048) 桶，取上界 2048");
        assert!(row.p50_ms < row.p99_ms);
    }
}
