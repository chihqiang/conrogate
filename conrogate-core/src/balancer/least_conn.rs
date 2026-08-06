//! LeastConnections 负载均衡算法。
//!
//! 连接计数按 upstream_id 隔离（负载均衡器注册表为全局单例，跨 upstream
//! 混用计数会导致同地址节点计数串扰）。

use crate::contract::balancer::{BalancerAlgorithm, LoadBalancer};
use crate::contract::dto::UpstreamNodeDto;
use crate::contract::ConrogateError;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct LeastConnections {
    // upstream_id → (节点地址 → 当前连接数)（内存维护，近似值）
    connections: Mutex<HashMap<u64, HashMap<String, u64>>>,
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

        let upstream_id = enabled[0].upstream_id;
        let mut conns = self.connections.lock().unwrap();

        // 清理已移除的节点（仅限当前 upstream）
        let current_addrs: std::collections::HashSet<&String> =
            enabled.iter().map(|n| &n.address).collect();
        if let Some(inner) = conns.get_mut(&upstream_id) {
            inner.retain(|addr, _| current_addrs.contains(addr));
        }

        let inner = conns.entry(upstream_id).or_default();
        // 选择连接数最少的节点
        let selected = enabled
            .iter()
            .min_by_key(|n| inner.get(&n.address).copied().unwrap_or(0))
            .copied()
            .unwrap();

        // 增加连接计数
        *inner.entry(selected.address.clone()).or_insert(0) += 1;

        Ok(selected.clone())
    }

    async fn release(&self, node: &UpstreamNodeDto, _key: Option<&str>) {
        let mut conns = self.connections.lock().unwrap();
        if let Some(inner) = conns.get_mut(&node.upstream_id) {
            if let Some(count) = inner.get_mut(&node.address) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    inner.remove(&node.address);
                }
            }
            // 该 upstream 无活跃连接时清理外层条目
            if inner.is_empty() {
                conns.remove(&node.upstream_id);
            }
        }
    }
}
