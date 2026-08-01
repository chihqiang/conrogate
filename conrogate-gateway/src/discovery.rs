//! 静态服务发现：从内存 ConfigSnapshot 读取节点列表。

use conrogate_contract::discovery::ServiceDiscovery;
use conrogate_contract::dto::{UpstreamDto, UpstreamNodeDto};
use conrogate_contract::ConrogateError;
use std::sync::RwLock;

/// 静态服务发现：从内存中的上游配置解析节点
pub struct StaticDiscovery {
    upstreams: RwLock<Vec<UpstreamDto>>,
}

impl StaticDiscovery {
    pub fn new() -> Self {
        Self {
            upstreams: RwLock::new(Vec::new()),
        }
    }

    /// 加载上游配置
    pub fn load(&self, upstreams: Vec<UpstreamDto>) {
        let mut guard = self.upstreams.write().unwrap();
        *guard = upstreams;
    }

    /// 根据 upstream_id 查找节点
    fn find_nodes(&self, upstream_id: u64) -> Vec<UpstreamNodeDto> {
        let guard = self.upstreams.read().unwrap();
        guard
            .iter()
            .find(|u| u.id == upstream_id)
            .map(|u| u.nodes.clone())
            .unwrap_or_default()
    }
}

impl Default for StaticDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ServiceDiscovery for StaticDiscovery {
    fn name(&self) -> &'static str {
        "static"
    }

    async fn resolve(
        &self,
        service_name: &str,
    ) -> Result<Vec<UpstreamNodeDto>, ConrogateError> {
        // service_name 格式为 "upstream:{id}"
        let upstream_id: u64 = service_name
            .strip_prefix("upstream:")
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| {
                ConrogateError::Internal(format!("invalid service name: {service_name}"))
            })?;

        Ok(self.find_nodes(upstream_id))
    }
}
