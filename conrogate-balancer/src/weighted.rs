//! WeightedRoundRobin 负载均衡算法（平滑加权轮询）。

use conrogate_contract::balancer::{BalancerAlgorithm, LoadBalancer};
use conrogate_contract::dto::UpstreamNodeDto;
use conrogate_contract::ConrogateError;
use std::sync::Mutex;

pub struct WeightedRoundRobin {
    // 当前权重快照，每次选择后更新
    current_weights: Mutex<Vec<i32>>,
    // 记录上次节点列表的长度，用于检测节点变化时重置
    last_len: Mutex<usize>,
}

impl WeightedRoundRobin {
    pub fn new() -> Self {
        Self {
            current_weights: Mutex::new(Vec::new()),
            last_len: Mutex::new(0),
        }
    }
}

impl Default for WeightedRoundRobin {
    fn default() -> Self {
        Self::new()
    }
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
        let enabled: Vec<&UpstreamNodeDto> = nodes.iter().filter(|n| n.enabled && n.weight > 0).collect();
        if enabled.is_empty() {
            return Err(ConrogateError::UpstreamNotFound("no enabled weighted nodes".into()));
        }

        let mut cw = self.current_weights.lock().unwrap();
        let mut last_len = self.last_len.lock().unwrap();

        // 节点列表变化时重置
        if *last_len != enabled.len() {
            *cw = vec![0; enabled.len()];
            *last_len = enabled.len();
        }

        // 平滑加权轮询算法
        // 1. 当前权重 += 有效权重
        // 2. 选择当前权重最大的节点
        // 3. 被选中的节点当前权重 -= 总权重
        let total_weight: i32 = enabled.iter().map(|n| n.weight).sum();

        for (i, node) in enabled.iter().enumerate() {
            cw[i] += node.weight;
        }

        let max_idx = cw
            .iter()
            .enumerate()
            .max_by_key(|(_, &w)| w)
            .map(|(i, _)| i)
            .unwrap_or(0);

        cw[max_idx] -= total_weight;

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
            UpstreamNodeDto { id: 1, upstream_id: 1, address: "10.0.0.1:8080".into(), weight: 5, enabled: true },
            UpstreamNodeDto { id: 2, upstream_id: 1, address: "10.0.0.2:8080".into(), weight: 1, enabled: true },
        ];

        let mut count_1 = 0;
        let mut count_2 = 0;
        for _ in 0..6 {
            let node = lb.select(&nodes, None).await.unwrap();
            if node.id == 1 { count_1 += 1; }
            if node.id == 2 { count_2 += 1; }
        }

        assert_eq!(count_1, 5);
        assert_eq!(count_2, 1);
    }
}
