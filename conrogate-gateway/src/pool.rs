//! 上游选择器：集成负载均衡 + 健康检查 + 服务发现。

use conrogate_contract::balancer::{BalancerAlgorithm, BalancerRegistry};
use conrogate_contract::dto::{RouteSnapshot, UpstreamDto, UpstreamNodeDto};
use conrogate_contract::gateway::UpstreamSelector;
use conrogate_contract::ConrogateError;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::health::PassiveHealthChecker;

/// 上游配置映射类型别名（避免 clippy type_complexity）
pub type UpstreamMap = HashMap<u64, (BalancerAlgorithm, Vec<UpstreamNodeDto>)>;

pub struct UpstreamSelectorImpl {
    registry: BalancerRegistry,
    // upstream_id → (algorithm, nodes)
    upstreams: Arc<RwLock<UpstreamMap>>,
    // 被动健康检查器（可选）
    health_checker: Option<Arc<PassiveHealthChecker>>,
}

impl UpstreamSelectorImpl {
    pub fn new(registry: BalancerRegistry) -> Self {
        Self {
            registry,
            upstreams: Arc::new(RwLock::new(HashMap::new())),
            health_checker: None,
        }
    }

    /// 设置被动健康检查器
    pub fn with_health_checker(mut self, hc: Arc<PassiveHealthChecker>) -> Self {
        self.health_checker = Some(hc);
        self
    }

    /// 获取上游节点映射的共享引用（用于主动健康检查等）
    pub fn shared_upstreams(&self) -> Arc<RwLock<UpstreamMap>> {
        self.upstreams.clone()
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
    fn get_nodes(
        &self,
        upstream_id: u64,
    ) -> Result<(BalancerAlgorithm, Vec<UpstreamNodeDto>), ConrogateError> {
        let upstreams = self.upstreams.read().unwrap();
        upstreams
            .get(&upstream_id)
            .cloned()
            .ok_or_else(|| ConrogateError::UpstreamNotFound(format!("upstream {}", upstream_id)))
    }
}

#[async_trait::async_trait]
impl UpstreamSelector for UpstreamSelectorImpl {
    async fn select_upstream(
        &self,
        route: &RouteSnapshot,
        key: Option<&str>,
    ) -> Result<UpstreamNodeDto, ConrogateError> {
        let upstream_id = route
            .upstream_id
            .ok_or_else(|| ConrogateError::UpstreamNotFound("route has no upstream".into()))?;

        let (algorithm, nodes) = self.get_nodes(upstream_id)?;

        // 过滤不健康节点（被动健康检查）
        let healthy_nodes = if let Some(ref hc) = self.health_checker {
            let filtered = hc.filter_healthy(&nodes).await;
            if filtered.is_empty() {
                // 所有节点都不健康，仍使用全部节点（避尼全部摘除）
                nodes
            } else {
                filtered
            }
        } else {
            nodes
        };

        let balancer = self
            .registry
            .get(algorithm)
            .ok_or_else(|| ConrogateError::Internal(format!("no balancer for {:?}", algorithm)))?;

        // 一致性哈希等按调用方传入的 key（client_ip）做亲和
        balancer.select(&healthy_nodes, key).await
    }

    async fn release_node(&self, route: &RouteSnapshot, node: &UpstreamNodeDto) {
        let upstream_id = match route.upstream_id {
            Some(id) => id,
            None => return,
        };
        let (algorithm, _) = match self.get_nodes(upstream_id) {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Some(balancer) = self.registry.get(algorithm) {
            balancer.release(node, route.host_header.as_deref()).await;
        }
    }
}
