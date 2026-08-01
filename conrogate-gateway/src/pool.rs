//! 上游选择器：集成负载均衡 + 健康检查 + 服务发现。

use conrogate_contract::balancer::{BalancerAlgorithm, BalancerRegistry, LoadBalancer};
use conrogate_contract::dto::{RouteSnapshot, UpstreamDto, UpstreamNodeDto};
use conrogate_contract::gateway::UpstreamSelector;
use conrogate_contract::ConrogateError;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub struct UpstreamSelectorImpl {
    registry: BalancerRegistry,
    // upstream_id → (algorithm, nodes)
    upstreams: RwLock<HashMap<u64, (BalancerAlgorithm, Vec<UpstreamNodeDto>)>>,
}

impl UpstreamSelectorImpl {
    pub fn new(registry: BalancerRegistry) -> Self {
        Self {
            registry,
            upstreams: RwLock::new(HashMap::new()),
        }
    }

    /// 加载上游配置
    pub fn load_upstreams(&self, upstreams: Vec<UpstreamDto>) {
        let mut map = self.upstreams.write().unwrap();
        map.clear();
        for up in upstreams {
            map.insert(up.id, (up.algorithm, up.nodes));
        }
    }

    /// 获取节点列表
    fn get_nodes(&self, upstream_id: u64) -> Result<(BalancerAlgorithm, Vec<UpstreamNodeDto>), ConrogateError> {
        let upstreams = self.upstreams.read().unwrap();
        upstreams
            .get(&upstream_id)
            .cloned()
            .ok_or_else(|| ConrogateError::UpstreamNotFound(format!("upstream {}", upstream_id)))
    }
}

#[async_trait::async_trait]
impl UpstreamSelector for UpstreamSelectorImpl {
    async fn select_upstream(&self, route: &RouteSnapshot) -> Result<UpstreamNodeDto, ConrogateError> {
        let upstream_id = route
            .upstream_id
            .ok_or_else(|| ConrogateError::UpstreamNotFound("route has no upstream".into()))?;

        let (algorithm, nodes) = self.get_nodes(upstream_id)?;

        let balancer = self
            .registry
            .get(algorithm)
            .ok_or_else(|| ConrogateError::Internal(format!("no balancer for {:?}", algorithm)))?;

        // 一致性哈希需要 key（如 client IP 或 session ID）
        let key = route.host_header.as_deref();

        balancer.select(&nodes, key).await
    }
}
