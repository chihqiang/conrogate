//! 超时控制实现。

use std::time::Duration;
use tokio::time::timeout;

/// 包装 Future 添加超时
pub async fn with_timeout<F>(dur: Duration, fut: F) -> Result<F::Output, TimeoutError>
where
    F: std::future::Future,
{
    timeout(dur, fut).await.map_err(|_| TimeoutError::Elapsed)
}

/// 超时错误
#[derive(Debug, Clone, Copy)]
pub enum TimeoutError {
    Elapsed,
}
