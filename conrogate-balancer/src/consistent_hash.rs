//! ConsistentHash 负载均衡算法。

use conrogate_contract::balancer::{BalancerAlgorithm, LoadBalancer};
use conrogate_contract::dto::UpstreamNodeDto;
use conrogate_contract::ConrogateError;
use std::collections::BTreeMap;

pub struct ConsistentHash {
    // 虚拟节点数
    vnodes: usize,
}

impl ConsistentHash {
    pub fn new() -> Self {
        Self { vnodes: 150 }
    }

    /// 设置虚拟节点数
    pub fn with_vnodes(mut self, vnodes: usize) -> Self {
        self.vnodes = vnodes.max(1);
        self
    }

    /// 简易哈希函数（FNV-1a）
    fn hash(s: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in s.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// 构建哈希环
    fn build_ring<'a>(&self, nodes: &[&'a UpstreamNodeDto]) -> BTreeMap<u64, &'a UpstreamNodeDto> {
        let mut ring = BTreeMap::new();
        for &node in nodes {
            for i in 0..self.vnodes {
                let key = format!("{}#{}", node.address, i);
                let hash = Self::hash(&key);
                ring.insert(hash, node);
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

        let ring = self.build_ring(&enabled);
        if ring.is_empty() {
            return Ok(enabled[0].clone());
        }

        let hash = Self::hash(key);

        // 顺时针查找第一个 >= hash 的节点
        let node = ring
            .range(hash..)
            .next()
            .or_else(|| ring.iter().next())
            .map(|(_, n)| (*n).clone())
            .unwrap();

        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_consistent_hash_same_key() {
        let lb = ConsistentHash::new();
        let nodes = vec![
            UpstreamNodeDto { id: 1, upstream_id: 1, address: "10.0.0.1:8080".into(), weight: 1, enabled: true },
            UpstreamNodeDto { id: 2, upstream_id: 1, address: "10.0.0.2:8080".into(), weight: 1, enabled: true },
            UpstreamNodeDto { id: 3, upstream_id: 1, address: "10.0.0.3:8080".into(), weight: 1, enabled: true },
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
            UpstreamNodeDto { id: 1, upstream_id: 1, address: "10.0.0.1:8080".into(), weight: 1, enabled: true },
            UpstreamNodeDto { id: 2, upstream_id: 1, address: "10.0.0.2:8080".into(), weight: 1, enabled: true },
            UpstreamNodeDto { id: 3, upstream_id: 1, address: "10.0.0.3:8080".into(), weight: 1, enabled: true },
        ];

        // 1000 个不同 key 应分散到多个节点
        let mut addrs = std::collections::HashSet::new();
        for i in 0..1000 {
            let node = lb.select(&nodes, Some(&format!("key-{i}"))).await.unwrap();
            addrs.insert(node.address);
        }
        assert!(addrs.len() >= 2, "should distribute to at least 2 nodes");
    }
}
