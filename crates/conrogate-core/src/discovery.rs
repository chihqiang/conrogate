//! 服务发现 Trait（扩展点）。

use crate::dto::UpstreamNodeDto;
use crate::error::ConrogateError;
use async_trait::async_trait;

/// 服务发现接口
/// 默认实现：StaticDiscovery（从数据库读取静态配置的节点）
/// 未来扩展：DnsDiscovery / ConsulDiscovery / K8sDiscovery
#[async_trait]
pub trait ServiceDiscovery: Send + Sync {
    fn name(&self) -> &'static str;

    async fn resolve(&self, service_name: &str) -> Result<Vec<UpstreamNodeDto>, ConrogateError>;
}
