//! TaskManager：后台任务注册 + 逆序取消。

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// 任务管理器：注册后台任务，优雅停机时逆序取消。
pub struct TaskManager {
    handles: Vec<JoinHandle<()>>,
    tokens: Vec<CancellationToken>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            handles: Vec::new(),
            tokens: Vec::new(),
        }
    }

    /// 注册一个后台任务，返回 CancellationToken 供任务内部监听
    pub fn spawn<F>(&mut self, name: &str, f: F) -> CancellationToken
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let token = CancellationToken::new();
        let child_token = token.clone();

        let handle = tokio::spawn(async move {
            // 等待取消信号或任务自然完成
            tokio::select! {
                _ = child_token.cancelled() => {
                    tracing::info!("task cancelled");
                }
                _ = f => {
                    // 任务自然完成
                }
            }
        });

        tracing::info!(task = name, "background task registered");
        self.handles.push(handle);
        self.tokens.push(token.clone());
        token
    }

    /// 逆序取消所有任务并等待完成（带超时）
    pub async fn shutdown(&mut self, timeout: std::time::Duration) {
        // 逆序取消
        for token in self.tokens.iter().rev() {
            token.cancel();
        }

        // 等待所有任务完成（带超时）
        let deadline = tokio::time::Instant::now() + timeout;
        for handle in self.handles.drain(..) {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                tracing::warn!("task shutdown timeout, aborting");
                handle.abort();
                continue;
            }
            let remaining = deadline - now;
            let _ = tokio::time::timeout(remaining, handle).await;
        }

        tracing::info!("all background tasks stopped");
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}
