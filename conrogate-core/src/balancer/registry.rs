//! 负载均衡器注册表。

use crate::contract::balancer::BalancerRegistry;
use std::sync::Arc;

/// 创建注册表并注册全部 4 种默认算法
pub fn create_default_registry() -> BalancerRegistry {
    let mut registry = BalancerRegistry::new();
    registry.register(Arc::new(crate::balancer::round_robin::RoundRobin::new()));
    registry.register(Arc::new(crate::balancer::weighted::WeightedRoundRobin::new()));
    registry.register(Arc::new(crate::balancer::least_conn::LeastConnections::new()));
    registry.register(Arc::new(crate::balancer::consistent_hash::ConsistentHash::new()));
    registry
}
