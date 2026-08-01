//! 健康检查 Trait（扩展点）。

use crate::dto::UpstreamNodeDto;
use crate::error::ConrogateError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeHealth {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
}

/// 健康检查接口
/// 默认实现：PassiveHealthChecker（基于快速失败标记的被动健康检查）
/// 未来扩展：ActiveHealthChecker（主动 HTTP/TCP 探测）
#[async_trait]
pub trait HealthChecker: Send + Sync {
    fn name(&self) -> &'static str;

    async fn check(
        &self,
        node: &UpstreamNodeDto,
    ) -> Result<NodeHealth, ConrogateError>;

    async fn mark_failure(&self, node_id: u64);

    async fn node_state(&self, node_id: u64) -> NodeHealth;
}
