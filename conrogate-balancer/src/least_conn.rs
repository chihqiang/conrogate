//! LeastConnections 负载均衡算法。

use conrogate_contract::balancer::{BalancerAlgorithm, LoadBalancer};
use conrogate_contract::dto::UpstreamNodeDto;
use conrogate_contract::ConrogateError;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct LeastConnections {
    // 节点地址 → 当前连接数（内存维护，近似值）
    connections: Mutex<HashMap<String, u64>>,
}

impl LeastConnections {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for LeastConnections {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LoadBalancer for LeastConnections {
    fn algorithm(&self) -> BalancerAlgorithm {
        BalancerAlgorithm::LeastConnections
    }

    async fn select(
        &self,
        nodes: &[UpstreamNodeDto],
        _key: Option<&str>,
    ) -> Result<UpstreamNodeDto, ConrogateError> {
        let enabled: Vec<&UpstreamNodeDto> = nodes.iter().filter(|n| n.enabled).collect();
        if enabled.is_empty() {
            return Err(ConrogateError::UpstreamNotFound("no enabled nodes".into()));
        }

        let mut conns = self.connections.lock().unwrap();

        // 清理已移除的节点
        let current_addrs: std::collections::HashSet<&String> =
            enabled.iter().map(|n| &n.address).collect();
        conns.retain(|addr, _| current_addrs.contains(addr));

        // 选择连接数最少的节点
        let selected = enabled
            .iter()
            .min_by_key(|n| conns.get(&n.address).copied().unwrap_or(0))
            .copied()
            .unwrap();

        // 增加连接计数
        *conns.entry(selected.address.clone()).or_insert(0) += 1;

        Ok(selected.clone())
    }

    async fn release(&self, node: &UpstreamNodeDto, _key: Option<&str>) {
        let mut conns = self.connections.lock().unwrap();
        if let Some(count) = conns.get_mut(&node.address) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                conns.remove(&node.address);
            }
        }
    }
}
