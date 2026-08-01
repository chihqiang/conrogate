//! 被动健康检查器：基于快速失败标记跳过故障节点。

use conrogate_contract::dto::UpstreamNodeDto;
use conrogate_contract::health::{HealthChecker, NodeHealth};
use conrogate_contract::ConrogateError;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 被动健康检查器
pub struct PassiveHealthChecker {
    // node_id → 失败计数 + 最后失败时间
    states: Mutex<HashMap<u64, NodeState>>,
    // 连续失败多少次标记为 Unhealthy
    failure_threshold: u32,
    // Unhealthy 后多久允许重试
    recovery_wait: Duration,
}

struct NodeState {
    consecutive_failures: u32,
    last_failure: Option<Instant>,
}

impl PassiveHealthChecker {
    pub fn new(failure_threshold: u32, recovery_wait: Duration) -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
            failure_threshold,
            recovery_wait,
        }
    }
}

impl Default for PassiveHealthChecker {
    fn default() -> Self {
        Self::new(3, Duration::from_secs(30))
    }
}

#[async_trait::async_trait]
impl HealthChecker for PassiveHealthChecker {
    fn name(&self) -> &'static str {
        "passive"
    }

    async fn check(
        &self,
        node: &UpstreamNodeDto,
    ) -> Result<NodeHealth, ConrogateError> {
        let states = self.states.lock().unwrap();
        match states.get(&node.id) {
            None => Ok(NodeHealth::Healthy),
            Some(state) => {
                if state.consecutive_failures >= self.failure_threshold {
                    // 检查是否已过恢复等待时间
                    if let Some(last_fail) = state.last_failure {
                        if last_fail.elapsed() >= self.recovery_wait {
                            return Ok(NodeHealth::Degraded {
                                reason: "recovery probe".into(),
                            });
                        }
                    }
                    Ok(NodeHealth::Unhealthy {
                        reason: format!(
                            "{} consecutive failures",
                            state.consecutive_failures
                        ),
                    })
                } else if state.consecutive_failures > 0 {
                    Ok(NodeHealth::Degraded {
                        reason: format!(
                            "{} recent failures",
                            state.consecutive_failures
                        ),
                    })
                } else {
                    Ok(NodeHealth::Healthy)
                }
            }
        }
    }

    async fn mark_failure(&self, node_id: u64) {
        let mut states = self.states.lock().unwrap();
        let state = states.entry(node_id).or_insert(NodeState {
            consecutive_failures: 0,
            last_failure: None,
        });
        state.consecutive_failures += 1;
        state.last_failure = Some(Instant::now());
    }

    async fn node_state(&self, node_id: u64) -> NodeHealth {
        let states = self.states.lock().unwrap();
        match states.get(&node_id) {
            None => NodeHealth::Healthy,
            Some(state) => {
                if state.consecutive_failures >= self.failure_threshold {
                    NodeHealth::Unhealthy {
                        reason: format!("{} failures", state.consecutive_failures),
                    }
                } else if state.consecutive_failures > 0 {
                    NodeHealth::Degraded {
                        reason: format!("{} failures", state.consecutive_failures),
                    }
                } else {
                    NodeHealth::Healthy
                }
            }
        }
    }
}

impl PassiveHealthChecker {
    /// 标记节点成功（重置失败计数）
    pub fn mark_success(&self, node_id: u64) {
        let mut states = self.states.lock().unwrap();
        if let Some(state) = states.get_mut(&node_id) {
            state.consecutive_failures = 0;
        }
    }

    /// 过滤健康节点
    pub async fn filter_healthy(
        &self,
        nodes: &[UpstreamNodeDto],
    ) -> Vec<UpstreamNodeDto> {
        let mut healthy = Vec::new();
        for node in nodes {
            if !node.enabled {
                continue;
            }
            match self.check(node).await {
                Ok(NodeHealth::Healthy) | Ok(NodeHealth::Degraded { .. }) => {
                    healthy.push(node.clone());
                }
                _ => {}
            }
        }
        healthy
    }
}
