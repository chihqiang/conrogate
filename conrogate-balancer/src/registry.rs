//! 负载均衡器注册表。

use conrogate_contract::balancer::{BalancerAlgorithm, BalancerRegistry, LoadBalancer};
use std::sync::Arc;

/// 创建注册表并注册全部 4 种默认算法
pub fn create_default_registry() -> BalancerRegistry {
    let mut registry = BalancerRegistry::new();
    registry.register(Arc::new(crate::round_robin::RoundRobin::new()));
    registry.register(Arc::new(crate::weighted::WeightedRoundRobin::new()));
    registry.register(Arc::new(crate::least_conn::LeastConnections::new()));
    registry.register(Arc::new(crate::consistent_hash::ConsistentHash::new()));
    registry
}
