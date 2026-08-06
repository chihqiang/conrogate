//! WeightedRoundRobin 负载均衡算法（平滑加权轮询）。
//!
//! 状态按节点集合签名隔离：不同 upstream（或节点增删/权重变化）的节点列表
//! 拥有独立的权重状态，避免注册表单例被多个 upstream 共享导致状态错乱。

use crate::contract::balancer::{BalancerAlgorithm, LoadBalancer};
use crate::contract::dto::UpstreamNodeDto;
use crate::contract::ConrogateError;
use std::collections::HashMap;
use std::sync::Mutex;

/// 状态表上限，防止节点集合频繁变化时状态无限累积
const MAX_STATES: usize = 1024;

pub struct WeightedRoundRobin {
    // 节点集合签名 → 当前权重快照（每次选择后更新）
    states: Mutex<HashMap<String, Vec<i32>>>,
}

impl WeightedRoundRobin {
    pub fn new() -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for WeightedRoundRobin {
    fn default() -> Self {
        Self::new()
    }
}

/// 节点集合签名：按 address:weight 排序拼接，用于识别节点列表是否变化
fn signature(nodes: &[&UpstreamNodeDto]) -> String {
    let mut parts: Vec<String> = nodes
        .iter()
        .map(|n| format!("{}:{}", n.address, n.weight))
        .collect();
    parts.sort();
    parts.join("|")
}

#[async_trait::async_trait]
impl LoadBalancer for WeightedRoundRobin {
    fn algorithm(&self) -> BalancerAlgorithm {
        BalancerAlgorithm::WeightedRoundRobin
    }

    async fn select(
        &self,
        nodes: &[UpstreamNodeDto],
        _key: Option<&str>,
    ) -> Result<UpstreamNodeDto, ConrogateError> {
        let enabled: Vec<&UpstreamNodeDto> =
            nodes.iter().filter(|n| n.enabled && n.weight > 0).collect();
        if enabled.is_empty() {
            return Err(ConrogateError::UpstreamNotFound(
                "no enabled weighted nodes".into(),
            ));
        }

        let mut states = self.states.lock().unwrap();
        let sig = signature(&enabled);

        // 状态表防无限增长：超限且新节点集合 → 整体重建（罕见事件，短暂失衡可接受）
        if states.len() >= MAX_STATES && !states.contains_key(&sig) {
            states.clear();
        }

        // 节点集合变化 → 重置权重快照
        let current_weights = states.entry(sig).or_insert_with(|| vec![0; enabled.len()]);

        // 平滑加权轮询算法
        // 1. 当前权重 += 有效权重
        // 2. 选择当前权重最大的节点
        // 3. 被选中的节点当前权重 -= 总权重
        let total_weight: i32 = enabled.iter().map(|n| n.weight).sum();

        for (i, node) in enabled.iter().enumerate() {
            current_weights[i] += node.weight;
        }

        let max_idx = current_weights
            .iter()
            .enumerate()
            .max_by_key(|(_, &w)| w)
            .map(|(i, _)| i)
            .unwrap_or(0);

        current_weights[max_idx] -= total_weight;

        Ok(enabled[max_idx].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_weighted_round_robin() {
        let lb = WeightedRoundRobin::new();
        let nodes = vec![
            UpstreamNodeDto {
                id: 1,
                upstream_id: 1,
                address: "10.0.0.1:8080".into(),
                weight: 5,
                enabled: true,
            },
            UpstreamNodeDto {
                id: 2,
                upstream_id: 1,
                address: "10.0.0.2:8080".into(),
                weight: 1,
                enabled: true,
            },
        ];

        let mut count_1 = 0;
        let mut count_2 = 0;
        for _ in 0..6 {
            let node = lb.select(&nodes, None).await.unwrap();
            if node.id == 1 {
                count_1 += 1;
            }
            if node.id == 2 {
                count_2 += 1;
            }
        }

        assert_eq!(count_1, 5);
        assert_eq!(count_2, 1);
    }

    #[tokio::test]
    async fn test_state_isolated_per_upstream() {
        let lb = WeightedRoundRobin::new();
        let nodes_a = vec![
            UpstreamNodeDto {
                id: 1,
                upstream_id: 1,
                address: "10.0.0.1:8080".into(),
                weight: 5,
                enabled: true,
            },
            UpstreamNodeDto {
                id: 2,
                upstream_id: 1,
                address: "10.0.0.2:8080".into(),
                weight: 1,
                enabled: true,
            },
        ];
        let nodes_b = vec![
            UpstreamNodeDto {
                id: 3,
                upstream_id: 2,
                address: "10.0.0.3:8080".into(),
                weight: 1,
                enabled: true,
            },
            UpstreamNodeDto {
                id: 4,
                upstream_id: 2,
                address: "10.0.0.4:8080".into(),
                weight: 1,
                enabled: true,
            },
        ];

        // upstream A：5:1 权重 → 每 6 次 5 次命中节点 1
        let mut count_a1 = 0;
        for _ in 0..6 {
            let node = lb.select(&nodes_a, None).await.unwrap();
            if node.id == 1 {
                count_a1 += 1;
            }
        }
        assert_eq!(count_a1, 5);

        // upstream B：1:1 权重 → 交替命中
        let mut count_b3 = 0;
        for _ in 0..6 {
            let node = lb.select(&nodes_b, None).await.unwrap();
            if node.id == 3 {
                count_b3 += 1;
            }
        }
        assert_eq!(count_b3, 3);

        // upstream A 的状态不应被 B 的调用污染，继续满足 5:1
        let mut count_a1_after = 0;
        for _ in 0..6 {
            let node = lb.select(&nodes_a, None).await.unwrap();
            if node.id == 1 {
                count_a1_after += 1;
            }
        }
        assert_eq!(count_a1_after, 5);
    }
}
