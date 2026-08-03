//! ConsistentHash 负载均衡算法。
//!
//! 哈希环按节点集合缓存，节点列表未变化时复用，避免每次请求重建 O(n) 的环。
//! 虚拟节点数按节点权重成比例分配（权重越高，落在其上的 key 越多）。

use conrogate_contract::balancer::{BalancerAlgorithm, LoadBalancer};
use conrogate_contract::dto::UpstreamNodeDto;
use conrogate_contract::ConrogateError;
use std::collections::BTreeMap;
use std::sync::Mutex;

pub struct ConsistentHash {
    // 基础虚拟节点数（按权重成比例分配）
    vnodes: usize,
    // 缓存的哈希环：节点集合签名 → 环（hash → 节点在 enabled 列表中的下标）
    cache: Mutex<Option<RingCache>>,
}

struct RingCache {
    signature: String,
    ring: BTreeMap<u64, usize>,
}

impl ConsistentHash {
    pub fn new() -> Self {
        Self {
            vnodes: 150,
            cache: Mutex::new(None),
        }
    }

    /// 设置基础虚拟节点数
    pub fn with_vnodes(mut self, vnodes: usize) -> Self {
        self.vnodes = vnodes.max(1);
        self
    }

    /// 哈希函数：使用 SeaHash（确定性、雪崩特性好，避免顺序 key 下 FNV 的分布病态）
    fn hash(s: &str) -> u64 {
        seahash::hash(s.as_bytes())
    }

    /// 节点集合签名：地址 + 权重 + 节点 ID，用于判断环是否需要重建
    fn signature(nodes: &[&UpstreamNodeDto]) -> String {
        let mut parts: Vec<String> = nodes
            .iter()
            .map(|n| format!("{}:{}:{}", n.id, n.address, n.weight))
            .collect();
        parts.sort();
        parts.join("|")
    }

    /// 构建哈希环（虚拟节点数按权重成比例分配）
    fn build_ring(&self, nodes: &[&UpstreamNodeDto]) -> BTreeMap<u64, usize> {
        let mut ring = BTreeMap::new();
        let max_weight = nodes
            .iter()
            .map(|n| n.weight.max(1))
            .max()
            .unwrap_or(1);
        for (idx, &node) in nodes.iter().enumerate() {
            let weight = node.weight.max(1);
            let count = (self.vnodes as u64 * weight as u64 / max_weight as u64).max(1) as usize;
            for i in 0..count {
                let key = format!("{}#{}#{}", node.address, node.id, i);
                ring.insert(Self::hash(&key), idx);
            }
        }
        ring
    }
}

impl Default for ConsistentHash {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LoadBalancer for ConsistentHash {
    fn algorithm(&self) -> BalancerAlgorithm {
        BalancerAlgorithm::ConsistentHash
    }

    async fn select(
        &self,
        nodes: &[UpstreamNodeDto],
        key: Option<&str>,
    ) -> Result<UpstreamNodeDto, ConrogateError> {
        let enabled: Vec<&UpstreamNodeDto> = nodes.iter().filter(|n| n.enabled).collect();
        if enabled.is_empty() {
            return Err(ConrogateError::UpstreamNotFound("no enabled nodes".into()));
        }

        // 无 key 时退化为轮询第一个节点
        let key = match key {
            Some(k) if !k.is_empty() => k,
            _ => return Ok(enabled[0].clone()),
        };

        // 节点集合未变化时复用缓存的环
        let signature = Self::signature(&enabled);
        let mut cache = self.cache.lock().unwrap();
        let needs_rebuild = cache
            .as_ref()
            .map(|c| c.signature != signature)
            .unwrap_or(true);
        if needs_rebuild {
            let ring = self.build_ring(&enabled);
            *cache = Some(RingCache { signature, ring });
        }
        let ring = &cache.as_ref().expect("ring cache just populated").ring;
        if ring.is_empty() {
            return Ok(enabled[0].clone());
        }

        let hash = Self::hash(key);

        // 顺时针查找第一个 >= hash 的节点
        let idx = ring
            .range(hash..)
            .next()
            .or_else(|| ring.iter().next())
            .map(|(_, &i)| i)
            .unwrap_or(0);

        Ok(enabled[idx].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_consistent_hash_same_key() {
        let lb = ConsistentHash::new();
        let nodes = vec![
            UpstreamNodeDto {
                id: 1,
                upstream_id: 1,
                address: "10.0.0.1:8080".into(),
                weight: 1,
                enabled: true,
            },
            UpstreamNodeDto {
                id: 2,
                upstream_id: 1,
                address: "10.0.0.2:8080".into(),
                weight: 1,
                enabled: true,
            },
            UpstreamNodeDto {
                id: 3,
                upstream_id: 1,
                address: "10.0.0.3:8080".into(),
                weight: 1,
                enabled: true,
            },
        ];

        // 相同 key 应选到相同节点
        let r1 = lb.select(&nodes, Some("user:12345")).await.unwrap();
        let r2 = lb.select(&nodes, Some("user:12345")).await.unwrap();
        assert_eq!(r1.address, r2.address);
    }

    #[tokio::test]
    async fn test_consistent_hash_distribution() {
        let lb = ConsistentHash::new();
        let nodes = vec![
            UpstreamNodeDto {
                id: 1,
                upstream_id: 1,
                address: "10.0.0.1:8080".into(),
                weight: 1,
                enabled: true,
            },
            UpstreamNodeDto {
                id: 2,
                upstream_id: 1,
                address: "10.0.0.2:8080".into(),
                weight: 1,
                enabled: true,
            },
            UpstreamNodeDto {
                id: 3,
                upstream_id: 1,
                address: "10.0.0.3:8080".into(),
                weight: 1,
                enabled: true,
            },
        ];

        // 1000 个不同 key 应分散到多个节点
        let mut addrs = std::collections::HashSet::new();
        for i in 0..1000 {
            let node = lb.select(&nodes, Some(&format!("key-{i}"))).await.unwrap();
            addrs.insert(node.address);
        }
        assert!(addrs.len() >= 2, "should distribute to at least 2 nodes");
    }

    #[tokio::test]
    async fn test_consistent_hash_honors_weight() {
        let lb = ConsistentHash::new();
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

        // 高权重节点应承担明显更多请求
        let mut count_1 = 0;
        for i in 0..2000 {
            let node = lb.select(&nodes, Some(&format!("key-{i}"))).await.unwrap();
            if node.id == 1 {
                count_1 += 1;
            }
        }
        let ratio = count_1 as f64 / 2000.0;
        assert!(
            ratio > 0.6,
            "weighted node should get majority, got ratio {ratio}"
        );
    }
}
