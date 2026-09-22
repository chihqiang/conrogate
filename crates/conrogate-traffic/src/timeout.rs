//! 超时控制：统一封装 `tokio::time::timeout` + 错误映射。
//!
//! 消除代理转发链路中重复的 `.map_err(|_| ConrogateError::UpstreamTimeout)` 模式，
//! 提供：
//! 1. [`with_timeout`]：包装 Future，超时返回 `ConrogateError::UpstreamTimeout`
//! 2. [`with_timeout_or`]：包装 Future，超时返回自定义错误
//! 3. [`TimeoutStage`]：分阶段超时控制（连接 / 响应头 / 响应体）

use conrogate_core::ConrogateError;
use std::future::Future;
use std::time::Duration;

/// 包装 Future 添加超时控制，超时返回 `ConrogateError::UpstreamTimeout`。
///
/// # 示例
/// ```ignore
/// let resp = with_timeout(timeout, client.request(req)).await?;
/// ```
pub async fn with_timeout<F>(dur: Duration, fut: F) -> Result<F::Output, ConrogateError>
where
    F: Future,
{
    tokio::time::timeout(dur, fut)
        .await
        .map_err(|_| ConrogateError::UpstreamTimeout)
}

/// 包装 Future 添加超时控制，超时返回自定义错误。
///
/// # 示例
/// ```ignore
/// let resp = with_timeout_or(timeout, fut, || ConrogateError::Internal("read body".into())).await?;
/// ```
pub async fn with_timeout_or<F, E>(dur: Duration, fut: F, on_timeout: impl FnOnce() -> E) -> Result<F::Output, E>
where
    F: Future,
{
    tokio::time::timeout(dur, fut)
        .await
        .map_err(|_| on_timeout())
}

/// 分阶段超时控制：连接、响应头、响应体可分别配置不同超时。
///
/// 典型场景：
/// - 连接超时（较短，如 3s）：快速判定上游不可达
/// - 响应头超时（中等，如 15s）：等待上游处理完成开始响应
/// - 响应体超时（较长，如 30s）：允许大文件慢速传输
#[derive(Debug, Clone)]
pub struct TimeoutStage {
    /// 连接超时
    pub connect: Duration,
    /// 响应头超时（从发送请求到收到响应头）
    pub read: Duration,
    /// 总超时（覆盖整个请求生命周期）
    pub total: Duration,
}

impl Default for TimeoutStage {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(3),
            read: Duration::from_secs(15),
            total: Duration::from_secs(30),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_with_timeout_ok() {
        let result = with_timeout(Duration::from_secs(1), async { 42 }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_with_timeout_elapsed() {
        let result = with_timeout(Duration::from_millis(10), async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            42
        })
        .await;
        assert!(matches!(result, Err(ConrogateError::UpstreamTimeout)));
    }

    #[tokio::test]
    async fn test_with_timeout_or_custom_error() {
        let result: Result<i32, &str> =
            with_timeout_or(Duration::from_millis(10), async {
                tokio::time::sleep(Duration::from_secs(1)).await;
                42
            }, || "timed out")
            .await;
        assert_eq!(result, Err("timed out"));
    }
}
