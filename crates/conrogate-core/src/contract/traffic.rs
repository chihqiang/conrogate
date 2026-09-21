//! 流量治理 Trait：限流器、熔断器、重试器、自适应并发、重试预算。

use crate::contract::error::ConrogateError;
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

    async fn acquire(&self, key: &str, limit: u32, window: Duration) -> Result<(), ConrogateError>;
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
    async fn get_or_create(&self, route_id: u64, node_id: u64) -> std::sync::Arc<dyn Breaker>;
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

// ── 自适应并发控制 ──

/// 自适应并发许可：基于 AIMD（Additive Increase / Multiplicative Decrease）
/// 算法动态调整并发上限。上游延迟低时线性增加并发，延迟超阈值时乘性减少。
///
/// 核心思想（SRE Handling Overload）：
/// - 慢比拒绝更危险——过载的上游会拖垮调用方
/// - 快速失败优于排队等待——超限直接返回 503
/// - 自适应——不依赖静态配置，随负载动态伸缩
#[async_trait]
pub trait AdaptiveConcurrency: Send + Sync {
    /// 尝试获取并发许可。
    /// 返回 `Ok(permit)` 表示获得许可，`Err` 表示并发已满应快速失败。
    /// `permit` 在 drop 时自动释放计数。
    async fn try_acquire(&self) -> Result<ConcurrencyPermit, ConrogateError>;

    /// 记录一次请求的延迟样本，驱动 AIMD 调整。
    /// `latency` 为该请求从开始到结束的实际耗时。
    /// `success` 表示请求是否成功（失败会触发更激进的减少）。
    fn record_latency(&self, latency: Duration, success: bool);

    /// 当前生效的并发上限
    fn current_limit(&self) -> usize;

    /// 当前在途并发数
    fn current_inflight(&self) -> usize;
}

/// 并发许可：drop 时自动减少在途计数。
pub struct ConcurrencyPermit {
    /// drop 时调用的释放回调
    on_drop: Box<dyn FnOnce() + Send + Sync>,
}

impl ConcurrencyPermit {
    pub fn new(on_drop: impl FnOnce() + Send + Sync + 'static) -> Self {
        Self {
            on_drop: Box::new(on_drop),
        }
    }
}

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        let on_drop = std::mem::replace(&mut self.on_drop, Box::new(|| {}));
        on_drop();
    }
}

// ── 重试预算 ──

/// 重试预算：限制全局重试比例，防止重试风暴加剧过载。
///
/// SRE 原则：重试本身也是负载。当系统过载时，重试会使情况更糟。
/// 重试预算按时间窗口统计：重试请求数 / 总请求数 不超过配置比例（如 10%）。
#[async_trait]
pub trait RetryBudget: Send + Sync {
    /// 尝试消费一个重试配额。
    /// 返回 `Ok(())` 表示预算充足可以重试，`Err` 表示预算耗尽应停止重试。
    fn try_consume(&self) -> Result<(), ConrogateError>;

    /// 记录一次原始请求（非重试），作为预算分母。
    fn record_request(&self);

    /// 当前窗口内的重试比例（0.0~1.0）
    fn current_ratio(&self) -> f64;

    /// 当前窗口内的总请求数
    fn current_total(&self) -> u64;

    /// 当前窗口内的重试数
    fn current_retries(&self) -> u64;
}
