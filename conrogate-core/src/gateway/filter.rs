//! 请求过滤器：限流拦截 + 熔断检查 + 配置刷新热载。

use crate::contract::config::Config;
use crate::contract::health::HealthChecker;
use crate::contract::ConrogateError;
use std::sync::Arc;
use std::sync::RwLock;

use crate::gateway::health::PassiveHealthChecker;

/// 配置热载管理器
pub struct ConfigReloader {
    current_config: RwLock<Arc<Config>>,
}

impl ConfigReloader {
    pub fn new(config: Config) -> Self {
        Self {
            current_config: RwLock::new(Arc::new(config)),
        }
    }

    /// 获取当前配置快照
    pub fn current(&self) -> Arc<Config> {
        self.current_config.read().unwrap().clone()
    }

    /// 热更新配置
    pub fn reload(&self, config: Config) {
        let mut guard = self.current_config.write().unwrap();
        *guard = Arc::new(config);
        tracing::info!("config reloaded");
    }
}

/// 流量治理适配器：整合限流 + 熔断 + 被动健康检查
pub struct TrafficControlAdapter {
    pub limiter: Arc<dyn crate::contract::traffic::Limiter>,
    pub breaker_factory: Arc<dyn crate::contract::traffic::BreakerFactory>,
    /// 是否启用限流
    pub rate_limit_enabled: bool,
    /// 是否启用熔断
    pub breaker_enabled: bool,
    /// 全局 QPS 上限（0 = 不限）
    pub global_qps: u32,
    /// 单路由 QPS 上限（0 = 不限）
    pub route_qps: u32,
    /// 单 IP QPS 上限（0 = 不限）
    pub ip_qps: u32,
    /// 被动健康检查器（可选）
    pub health_checker: Option<Arc<PassiveHealthChecker>>,
}

impl TrafficControlAdapter {
    /// 使用默认 QPS 值创建（向后兼容，默认启用治理）
    pub fn new(
        limiter: Arc<dyn crate::contract::traffic::Limiter>,
        breaker_factory: Arc<dyn crate::contract::traffic::BreakerFactory>,
    ) -> Self {
        Self {
            limiter,
            breaker_factory,
            rate_limit_enabled: true,
            breaker_enabled: true,
            global_qps: 1000,
            route_qps: 200,
            ip_qps: 100,
            health_checker: None,
        }
    }

    /// 从限流 + 熔断配置创建（尊重 enabled 开关，QPS=0 表示不限）
    pub fn with_governance_config(
        limiter: Arc<dyn crate::contract::traffic::Limiter>,
        breaker_factory: Arc<dyn crate::contract::traffic::BreakerFactory>,
        rate_limit: &crate::contract::config::RateLimitConfig,
        breaker: &crate::contract::config::BreakerConfig,
    ) -> Self {
        Self {
            limiter,
            breaker_factory,
            rate_limit_enabled: rate_limit.enabled,
            breaker_enabled: breaker.enabled,
            global_qps: rate_limit.global_qps,
            route_qps: rate_limit.route_qps,
            ip_qps: rate_limit.ip_qps,
            health_checker: None,
        }
    }

    /// 设置被动健康检查器
    pub fn with_health_checker(mut self, hc: Arc<PassiveHealthChecker>) -> Self {
        self.health_checker = Some(hc);
        self
    }
}

#[async_trait::async_trait]
impl crate::contract::gateway::TrafficControl for TrafficControlAdapter {
    async fn check_rate_limit(&self, route_id: u64, client_ip: &str) -> Result<(), ConrogateError> {
        // 开关关闭：不限流
        if !self.rate_limit_enabled {
            return Ok(());
        }

        let window = std::time::Duration::from_secs(1);

        // 1. 全局 QPS 限流（0 = 不限）
        if self.global_qps > 0 {
            let global_key = "rate:global".to_string();
            self.limiter
                .acquire(&global_key, self.global_qps, window)
                .await?;
        }

        // 2. 单路由 QPS 限流（0 = 不限）
        if self.route_qps > 0 {
            let route_key = format!("rate:route:{route_id}");
            self.limiter
                .acquire(&route_key, self.route_qps, window)
                .await?;
        }

        // 3. 单路由+单 IP QPS 限流（0 = 不限）
        if self.ip_qps > 0 {
            let ip_key = format!("rate:{route_id}:{client_ip}");
            self.limiter.acquire(&ip_key, self.ip_qps, window).await?;
        }

        Ok(())
    }

    async fn check_circuit_breaker(
        &self,
        route_id: u64,
        node_id: u64,
    ) -> Result<(), ConrogateError> {
        // 开关关闭：不熔断
        if !self.breaker_enabled {
            return Ok(());
        }
        let breaker = self.breaker_factory.get_or_create(route_id, node_id).await;
        breaker.allow().await
    }

    async fn record_result(&self, route_id: u64, node_id: u64, success: bool) {
        // 熔断开启时反馈计数；被动健康检查始终反馈
        if self.breaker_enabled {
            let breaker = self.breaker_factory.get_or_create(route_id, node_id).await;
            if success {
                breaker.record_success().await;
            } else {
                breaker.record_failure().await;
            }
        }
        // 被动健康检查器反馈
        if let Some(ref hc) = self.health_checker {
            if success {
                hc.mark_success(node_id);
            } else {
                hc.mark_failure(node_id).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::config::{BreakerConfig as ContractBreakerConfig, RateLimitConfig};
    use crate::contract::gateway::TrafficControl;
    use crate::contract::traffic::BreakerFactory;
    use crate::traffic::breaker::{BreakerConfig, BreakerFactoryImpl};
    use crate::traffic::limiter::FixedWindowLimiter;
    use std::time::Duration;

    fn rate_limit(enabled: bool, global_qps: u32) -> RateLimitConfig {
        RateLimitConfig {
            enabled,
            global_qps,
            route_qps: 10,
            ip_qps: 10,
            ..Default::default()
        }
    }

    fn adapter(rate: RateLimitConfig, breaker: ContractBreakerConfig) -> TrafficControlAdapter {
        TrafficControlAdapter::with_governance_config(
            Arc::new(FixedWindowLimiter::new()),
            Arc::new(BreakerFactoryImpl::default()),
            &rate,
            &breaker,
        )
    }

    #[tokio::test]
    async fn test_rate_limit_disabled_bypasses_all() {
        let adapter = adapter(rate_limit(false, 1), ContractBreakerConfig::default());
        for _ in 0..100 {
            assert!(adapter.check_rate_limit(1, "1.2.3.4").await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_rate_limit_qps_zero_means_unlimited() {
        let mut rate = rate_limit(true, 0);
        rate.route_qps = 0;
        rate.ip_qps = 0;
        let adapter = adapter(rate, ContractBreakerConfig::default());
        for _ in 0..100 {
            assert!(adapter.check_rate_limit(1, "1.2.3.4").await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_rate_limit_applies_when_enabled() {
        let adapter = adapter(rate_limit(true, 1), ContractBreakerConfig::default());
        assert!(adapter.check_rate_limit(1, "1.2.3.4").await.is_ok());
        assert!(adapter.check_rate_limit(1, "1.2.3.4").await.is_err());
    }

    #[tokio::test]
    async fn test_breaker_disabled_bypasses_open_circuit() {
        let breaker_config = BreakerConfig {
            window: Duration::from_secs(10),
            failure_rate_threshold: 0.1,
            min_requests: 2,
            wait: Duration::from_secs(60),
            half_open_max: 1,
            redis_url: None,
        };
        let factory = Arc::new(BreakerFactoryImpl::new(breaker_config));
        let adapter = TrafficControlAdapter::with_governance_config(
            Arc::new(FixedWindowLimiter::new()),
            factory.clone(),
            &rate_limit(false, 1),
            &ContractBreakerConfig {
                enabled: false,
                ..Default::default()
            },
        );

        // 制造 Open 状态
        let breaker = factory.get_or_create(1, 1).await;
        breaker.allow().await.unwrap();
        breaker.record_failure().await;
        breaker.allow().await.unwrap();
        breaker.record_failure().await;
        assert!(breaker.allow().await.is_err());
        assert_eq!(
            breaker.state(),
            crate::contract::traffic::BreakerState::Open
        );

        // 熔断开关关闭 → 放行
        assert!(adapter.check_circuit_breaker(1, 1).await.is_ok());
    }
}
