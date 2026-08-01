//! 熔断器实现：Closed → Open → HalfOpen 状态机。

use conrogate_contract::traffic::{Breaker, BreakerFactory, BreakerState};
use conrogate_contract::ConrogateError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct BreakerInner {
    state: BreakerState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
}

pub struct BreakerImpl {
    inner: Mutex<BreakerInner>,
    config: BreakerConfig,
}

#[derive(Clone)]
pub struct BreakerConfig {
    pub failure_rate_threshold: f64,
    pub min_requests: u32,
    pub wait: Duration,
    pub half_open_max: u32,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        Self {
            failure_rate_threshold: 0.5,
            min_requests: 10,
            wait: Duration::from_secs(30),
            half_open_max: 5,
        }
    }
}

impl BreakerImpl {
    pub fn new(config: BreakerConfig) -> Self {
        Self {
            inner: Mutex::new(BreakerInner {
                state: BreakerState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
            }),
            config,
        }
    }
}

#[async_trait::async_trait]
impl Breaker for BreakerImpl {
    fn state(&self) -> BreakerState {
        self.inner.lock().unwrap().state
    }

    async fn allow(&self) -> Result<(), ConrogateError> {
        let mut inner = self.inner.lock().unwrap();

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
                if inner.success_count + inner.failure_count < self.config.half_open_max {
                    Ok(())
                } else {
                    Err(ConrogateError::CircuitBreakerOpen)
                }
            }
        }
    }

    async fn record_success(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.success_count += 1;

        match inner.state {
            BreakerState::HalfOpen => {
                // HalfOpen 阶段成功次数达到阈值 → 回到 Closed
                if inner.success_count >= self.config.half_open_max {
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

    async fn record_failure(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.failure_count += 1;
        inner.last_failure_time = Some(Instant::now());

        match inner.state {
            BreakerState::Closed => {
                // 判断失败率是否超过阈值
                let total = inner.success_count + inner.failure_count;
                if total >= self.config.min_requests {
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
        upstream_id: u64,
    ) -> Arc<dyn Breaker> {
        let mut breakers = self.breakers.lock().unwrap();
        breakers
            .entry((route_id, upstream_id))
            .or_insert_with(|| Arc::new(BreakerImpl::new(self.config.clone())))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_breaker_opens_on_failures() {
        let config = BreakerConfig {
            failure_rate_threshold: 0.5,
            min_requests: 4,
            wait: Duration::from_secs(1),
            half_open_max: 2,
        };
        let breaker = BreakerImpl::new(config);

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
}
