//! 流量治理 Trait：限流器、熔断器、重试器。

use crate::error::ConrogateError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ── 限流器 ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitAlgorithm {
    FixedWindow,
    SlidingWindow,
    TokenBucket,
}

#[async_trait]
pub trait Limiter: Send + Sync {
    fn algorithm(&self) -> LimitAlgorithm;

    async fn acquire(
        &self,
        key: &str,
        limit: u32,
        window: Duration,
    ) -> Result<(), ConrogateError>;
}

// ── 熔断器 ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[async_trait]
pub trait Breaker: Send + Sync {
    fn state(&self) -> BreakerState;

    async fn allow(&self) -> Result<(), ConrogateError>;

    async fn record_success(&self);

    async fn record_failure(&self);
}

#[async_trait]
pub trait BreakerFactory: Send + Sync {
    /// 按维度（route + 上游节点）获取或创建熔断器实例
    async fn get_or_create(
        &self,
        route_id: u64,
        node_id: u64,
    ) -> std::sync::Arc<dyn Breaker>;
}

// ── 重试器 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_jitter_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 2,
            base_jitter_ms: 50,
        }
    }
}

#[async_trait]
pub trait Retryer: Send + Sync {
    fn can_retry(&self, method: &str, allow_non_idempotent: bool) -> bool;

    fn next_backoff(&self, attempt: u32) -> Duration;
}
