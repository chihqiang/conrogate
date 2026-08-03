//! RoundRobin 负载均衡算法。

use conrogate_contract::balancer::{BalancerAlgorithm, LoadBalancer};
use conrogate_contract::dto::UpstreamNodeDto;
use conrogate_contract::ConrogateError;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct RoundRobin {
    counter: AtomicUsize,
}

impl RoundRobin {
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl Default for RoundRobin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LoadBalancer for RoundRobin {
    fn algorithm(&self) -> BalancerAlgorithm {
        BalancerAlgorithm::RoundRobin
    }

    async fn select(
        &self,
        nodes: &[UpstreamNodeDto],
        _key: Option<&str>,
    ) -> Result<UpstreamNodeDto, ConrogateError> {
        if nodes.is_empty() {
            return Err(ConrogateError::UpstreamNotFound(
                "no nodes available".into(),
            ));
        }

        let enabled: Vec<&UpstreamNodeDto> = nodes.iter().filter(|n| n.enabled).collect();
        if enabled.is_empty() {
            return Err(ConrogateError::UpstreamNotFound("no enabled nodes".into()));
        }

        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % enabled.len();
        Ok(enabled[idx].clone())
    }
}
