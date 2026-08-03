//! 负载均衡 Trait 与算法枚举。

use crate::dto::UpstreamNodeDto;
use crate::error::ConrogateError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BalancerAlgorithm {
    RoundRobin,
    WeightedRoundRobin,
    LeastConnections,
    ConsistentHash,
}

impl std::fmt::Display for BalancerAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RoundRobin => write!(f, "round_robin"),
            Self::WeightedRoundRobin => write!(f, "weighted_round_robin"),
            Self::LeastConnections => write!(f, "least_connections"),
            Self::ConsistentHash => write!(f, "consistent_hash"),
        }
    }
}

impl std::str::FromStr for BalancerAlgorithm {
    type Err = ConrogateError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "round_robin" => Ok(Self::RoundRobin),
            "weighted_round_robin" => Ok(Self::WeightedRoundRobin),
            "least_connections" => Ok(Self::LeastConnections),
            "consistent_hash" => Ok(Self::ConsistentHash),
            _ => Err(ConrogateError::BadRequest(format!(
                "unknown algorithm: {s}"
            ))),
        }
    }
}

/// 负载均衡器接口
#[async_trait]
pub trait LoadBalancer: Send + Sync {
    fn algorithm(&self) -> BalancerAlgorithm;

    async fn select(
        &self,
        nodes: &[UpstreamNodeDto],
        key: Option<&str>,
    ) -> Result<UpstreamNodeDto, ConrogateError>;

    /// 释放节点（连接结束/请求完成时递减计数）。
    /// 有状态算法（LeastConnections）需覆写，无状态算法使用默认空实现。
    async fn release(&self, _node: &UpstreamNodeDto, _key: Option<&str>) {}
}

/// 负载均衡器注册表
pub struct BalancerRegistry {
    balancers: HashMap<BalancerAlgorithm, Arc<dyn LoadBalancer>>,
}

impl BalancerRegistry {
    pub fn new() -> Self {
        Self {
            balancers: HashMap::new(),
        }
    }

    pub fn register(&mut self, balancer: Arc<dyn LoadBalancer>) {
        self.balancers.insert(balancer.algorithm(), balancer);
    }

    pub fn get(&self, algo: BalancerAlgorithm) -> Option<Arc<dyn LoadBalancer>> {
        self.balancers.get(&algo).cloned()
    }
}

impl Default for BalancerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
