//! 上游选择器：集成负载均衡 + 健康检查 + 服务发现。

use crate::contract::balancer::{BalancerAlgorithm, BalancerRegistry};
use crate::contract::dto::{RouteSnapshot, UpstreamDto, UpstreamNodeDto};
use crate::contract::gateway::UpstreamSelector;
use crate::contract::ConrogateError;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::gateway::health::PassiveHealthChecker;

/// 上游配置映射类型别名（避免 clippy type_complexity）
pub type UpstreamMap = HashMap<u64, (BalancerAlgorithm, Vec<UpstreamNodeDto>)>;

pub struct UpstreamSelectorImpl {
    registry: BalancerRegistry,
    // upstream_id → (algorithm, nodes)
    // 读取路径无锁化（Arc 整体替换）：select_upstream 原子 clone 快照后借用节点列表，
    // 避免每请求 clone 整份节点；配置热载整体替换。
    upstreams: Arc<RwLock<Arc<UpstreamMap>>>,
    // 被动健康检查器（可选）
    health_checker: Option<Arc<PassiveHealthChecker>>,
}

impl UpstreamSelectorImpl {
    pub fn new(registry: BalancerRegistry) -> Self {
        Self {
            registry,
            upstreams: Arc::new(RwLock::new(Arc::new(HashMap::new()))),
            health_checker: None,
        }
    }

    /// 设置被动健康检查器
    pub fn with_health_checker(mut self, hc: Arc<PassiveHealthChecker>) -> Self {
        self.health_checker = Some(hc);
        self
    }

    /// 获取上游节点映射的共享引用（用于主动健康检查等）
    pub fn shared_upstreams(&self) -> Arc<RwLock<Arc<UpstreamMap>>> {
        self.upstreams.clone()
    }

    /// 加载上游配置（整体构建后原子替换）
    pub fn load_upstreams(&self, upstreams: Vec<UpstreamDto>) {
        let mut map = HashMap::new();
        for up in upstreams {
            map.insert(up.id, (up.algorithm, up.nodes));
        }
        *self.upstreams.write().unwrap() = Arc::new(map);
    }

    /// 仅读取负载均衡算法（Copy，避免整份节点列表克隆）
    fn get_algorithm(&self, upstream_id: u64) -> Result<BalancerAlgorithm, ConrogateError> {
        let upstreams = self.upstreams.read().unwrap();
        upstreams
            .get(&upstream_id)
            .map(|(algo, _)| *algo)
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

        // 无锁快照：Arc clone 后直接借用节点列表（零克隆，跨 await 安全）
        let snapshot = Arc::clone(&self.upstreams.read().unwrap());
        let (algorithm, nodes) = snapshot
            .get(&upstream_id)
            .map(|(algo, nodes)| (*algo, nodes.as_slice()))
            .ok_or_else(|| ConrogateError::UpstreamNotFound(format!("upstream {}", upstream_id)))?;

        // 过滤不健康节点（被动健康检查）；无检查器时零克隆
        let filtered: Vec<UpstreamNodeDto>;
        let healthy_nodes = if let Some(ref hc) = self.health_checker {
            filtered = hc.filter_healthy(nodes).await;
            if filtered.is_empty() {
                // 所有节点都不健康，仍使用全部节点（避尼全部摘除）
                nodes
            } else {
                &filtered
            }
        } else {
            nodes
        };

        let balancer = self
            .registry
            .get(algorithm)
            .ok_or_else(|| ConrogateError::Internal(format!("no balancer for {:?}", algorithm)))?;

        // 一致性哈希等按调用方传入的 key（client_ip）做亲和
        balancer.select(healthy_nodes, key).await
    }

    async fn release_node(&self, route: &RouteSnapshot, node: &UpstreamNodeDto) {
        let upstream_id = match route.upstream_id {
            Some(id) => id,
            None => return,
        };
        let algorithm = match self.get_algorithm(upstream_id) {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Some(balancer) = self.registry.get(algorithm) {
            balancer.release(node, route.host_header.as_deref()).await;
        }
    }
}
