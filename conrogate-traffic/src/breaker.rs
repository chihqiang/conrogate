//! 熔断器实现：Closed → Open → HalfOpen 状态机 + 滑动窗口计数。
//!
//! 维度：route + 上游节点（docs/10 §9.2）。计数在 `window` 窗口内有效，
//! 窗口过期自动清零（docs/10 §9.1 计数窗口）。`mode=cluster` 时计数
//! 同步镜像到 Redis（docs/10 §9.3），判定仍为进程内状态机。

use conrogate_contract::traffic::{Breaker, BreakerFactory, BreakerState};
use conrogate_contract::ConrogateError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct BreakerInner {
    state: BreakerState,
    failure_count: u64,
    success_count: u64,
    window_start: Instant,
    last_failure_time: Option<Instant>,
}

pub struct BreakerImpl {
    inner: Mutex<BreakerInner>,
    config: BreakerConfig,
    key: String,
}

#[derive(Clone)]
pub struct BreakerConfig {
    pub window: Duration,
    pub failure_rate_threshold: f64,
    pub min_requests: u32,
    pub wait: Duration,
    pub half_open_max: u32,
    /// 集群模式 Redis URL（None = 单机）
    pub redis_url: Option<String>,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        Self {
            window: Duration::from_secs(10),
            failure_rate_threshold: 0.5,
            min_requests: 10,
            wait: Duration::from_secs(30),
            half_open_max: 5,
            redis_url: None,
        }
    }
}

impl BreakerImpl {
    pub fn new(config: BreakerConfig, key: String) -> Self {
        Self {
            inner: Mutex::new(BreakerInner {
                state: BreakerState::Closed,
                failure_count: 0,
                success_count: 0,
                window_start: Instant::now(),
                last_failure_time: None,
            }),
            config,
            key,
        }
    }

    /// 滑动窗口：窗口过期则重置计数
    fn refresh_window(inner: &mut BreakerInner, window: Duration) {
        if inner.window_start.elapsed() >= window {
            inner.failure_count = 0;
            inner.success_count = 0;
            inner.window_start = Instant::now();
        }
    }

    /// 集群模式：把计数镜像到 Redis（best-effort，不影响本地判定）
    async fn mirror_to_redis(&self, succ: bool) {
        let Some(ref url) = self.config.redis_url else { return };
        let client = match redis::Client::open(url.as_str()) {
            Ok(c) => c,
            Err(_) => return,
        };
        let mut conn = match client.get_multiplexed_async_connection().await {
            Ok(c) => c,
            Err(_) => return,
        };
        let (key, dir) = if succ {
            (format!("cb:{}:succ", self.key), 1)
        } else {
            (format!("cb:{}:fail", self.key), -1)
        };
        let window_secs = self.config.window.as_secs().max(1);
        let script = r#"
local k = KEYS[1]
local current = redis.call('INCRBY', k, ARGV[1])
if current == ARGV[1] then
    redis.call('EXPIRE', k, ARGV[2])
end
return current
"#;
        let _: Result<i64, _> = redis::cmd("EVAL")
            .arg(script)
            .arg(1)
            .arg(&key)
            .arg(dir)
            .arg(window_secs)
            .query_async(&mut conn)
            .await;
    }
}

#[async_trait::async_trait]
impl Breaker for BreakerImpl {
    fn state(&self) -> BreakerState {
        self.inner.lock().unwrap().state
    }

    async fn allow(&self) -> Result<(), ConrogateError> {
        let mut inner = self.inner.lock().unwrap();
        Self::refresh_window(&mut inner, self.config.window);

        match inner.state {
            BreakerState::Closed => Ok(()),
            BreakerState::Open => {
                // 检查是否到了可以尝试 HalfOpen 的时间
                if let Some(last_failure) = inner.last_failure_time {
                    if last_failure.elapsed() >= self.config.wait {
                        inner.state = BreakerState::HalfOpen;
                        inner.success_count = 0;
                        inner.failure_count = 0;
                        return Ok(());
                    }
                }
                Err(ConrogateError::CircuitBreakerOpen)
            }
            BreakerState::HalfOpen => {
                // HalfOpen 阶段允许有限请求通过
                if inner.success_count + inner.failure_count < self.config.half_open_max as u64 {
                    Ok(())
                } else {
                    Err(ConrogateError::CircuitBreakerOpen)
                }
            }
        }
    }

    async fn record_success(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            Self::refresh_window(&mut inner, self.config.window);
            inner.success_count += 1;

            match inner.state {
                BreakerState::HalfOpen => {
                    // HalfOpen 阶段成功次数达到阈值 → 回到 Closed
                    if inner.success_count >= self.config.half_open_max as u64 {
                        inner.state = BreakerState::Closed;
                        inner.failure_count = 0;
                        inner.success_count = 0;
                    }
                }
                BreakerState::Closed => {
                    // 正常运行中成功 → 重置计数
                    inner.failure_count = 0;
                }
                _ => {}
            }
        }
        self.mirror_to_redis(true).await;
    }

    async fn record_failure(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            Self::refresh_window(&mut inner, self.config.window);
            inner.failure_count += 1;
            inner.last_failure_time = Some(Instant::now());

            match inner.state {
                BreakerState::Closed => {
                    // 判断失败率是否超过阈值
                    let total = inner.success_count + inner.failure_count;
                    if total >= self.config.min_requests as u64 {
                        let failure_rate = inner.failure_count as f64 / total as f64;
                        if failure_rate >= self.config.failure_rate_threshold {
                            inner.state = BreakerState::Open;
                        }
                    }
                }
                BreakerState::HalfOpen => {
                    // HalfOpen 阶段出现失败 → 立即回到 Open
                    inner.state = BreakerState::Open;
                }
                _ => {}
            }
        }
        self.mirror_to_redis(false).await;
    }
}

// ── BreakerFactory 实现 ──

pub struct BreakerFactoryImpl {
    breakers: Mutex<HashMap<(u64, u64), Arc<dyn Breaker>>>,
    config: BreakerConfig,
}

impl BreakerFactoryImpl {
    pub fn new(config: BreakerConfig) -> Self {
        Self {
            breakers: Mutex::new(HashMap::new()),
            config,
        }
    }
}

impl Default for BreakerFactoryImpl {
    fn default() -> Self {
        Self::new(BreakerConfig::default())
    }
}

#[async_trait::async_trait]
impl BreakerFactory for BreakerFactoryImpl {
    async fn get_or_create(
        &self,
        route_id: u64,
        node_id: u64,
    ) -> Arc<dyn Breaker> {
        let mut breakers = self.breakers.lock().unwrap();
        breakers
            .entry((route_id, node_id))
            .or_insert_with(|| {
                let key = format!("{route_id}:{node_id}");
                Arc::new(BreakerImpl::new(self.config.clone(), key))
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_breaker_opens_on_failures() {
        let config = BreakerConfig {
            window: Duration::from_secs(10),
            failure_rate_threshold: 0.5,
            min_requests: 4,
            wait: Duration::from_secs(1),
            half_open_max: 2,
            redis_url: None,
        };
        let breaker = BreakerImpl::new(config, "1:2".into());

        // 2 成功 + 2 失败 = 50% 失败率 → 触发 Open
        breaker.allow().await.unwrap();
        breaker.record_success().await;
        breaker.allow().await.unwrap();
        breaker.record_success().await;
        breaker.allow().await.unwrap();
        breaker.record_failure().await;
        breaker.allow().await.unwrap();
        breaker.record_failure().await;

        assert_eq!(breaker.state(), BreakerState::Open);
        assert!(breaker.allow().await.is_err());
    }

    #[tokio::test]
    async fn test_window_clears_counts() {
        let config = BreakerConfig {
            window: Duration::from_millis(50),
            failure_rate_threshold: 0.5,
            min_requests: 4,
            wait: Duration::from_secs(1),
            half_open_max: 2,
            redis_url: None,
        };
        let breaker = BreakerImpl::new(config, "1:2".into());

        for _ in 0..2 {
            breaker.record_failure().await;
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
        // 窗口过期 → 计数清零，不再触发 Open
        assert_eq!(breaker.state(), BreakerState::Closed);
    }
}
