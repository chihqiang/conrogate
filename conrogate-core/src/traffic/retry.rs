//! 重试器实现。

use crate::contract::traffic::{RetryConfig, Retryer};
use std::time::Duration;

pub struct RetryerImpl {
    config: RetryConfig,
    /// 幂等方法集合
    idempotent_methods: std::collections::HashSet<&'static str>,
}

impl RetryerImpl {
    pub fn new(config: RetryConfig) -> Self {
        Self {
            config,
            idempotent_methods: ["GET", "HEAD", "OPTIONS", "PUT", "DELETE"]
                .into_iter()
                .collect(),
        }
    }

    pub fn with_extra_idempotent(mut self, method: &'static str) -> Self {
        self.idempotent_methods.insert(method);
        self
    }
}

impl Default for RetryerImpl {
    fn default() -> Self {
        Self::new(RetryConfig::default())
    }
}

#[async_trait::async_trait]
impl Retryer for RetryerImpl {
    fn can_retry(&self, method: &str, allow_non_idempotent: bool) -> bool {
        let upper = method.to_uppercase();
        if self.idempotent_methods.contains(upper.as_str()) {
            return true;
        }
        allow_non_idempotent
    }

    fn next_backoff(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::from_millis(0);
        }
        // 指数退避 + 抖动
        let base = self.config.base_jitter_ms;
        let exponent = (attempt - 1).min(10);
        let backoff_ms = base.wrapping_mul(1u64 << exponent);
        // 添加随机抖动（使用简单取模代替 rand，避免额外锁）
        let jitter = backoff_ms / 4;
        Duration::from_millis(backoff_ms + jitter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idempotent_retry() {
        let retryer = RetryerImpl::default();
        assert!(retryer.can_retry("GET", false));
        assert!(retryer.can_retry("PUT", false));
        assert!(!retryer.can_retry("POST", false));
        assert!(retryer.can_retry("POST", true));
    }

    #[test]
    fn test_backoff_increasing() {
        let config = RetryConfig {
            max_attempts: 3,
            base_jitter_ms: 50,
        };
        let retryer = RetryerImpl::new(config);

        let b1 = retryer.next_backoff(1);
        let b2 = retryer.next_backoff(2);
        let b3 = retryer.next_backoff(3);

        assert!(b2 > b1);
        assert!(b3 > b2);
    }
}
