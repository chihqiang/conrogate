//! 请求过滤器：限流拦截 + 熔断检查 + 配置刷新热载。

use conrogate_contract::config::Config;
use conrogate_contract::ConrogateError;
use conrogate_contract::health::HealthChecker;
use std::sync::Arc;
use std::sync::RwLock;

use crate::health::PassiveHealthChecker;

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
    pub limiter: Arc<dyn conrogate_contract::traffic::Limiter>,
    pub breaker_factory: Arc<dyn conrogate_contract::traffic::BreakerFactory>,
    /// 全局 QPS 上限
    pub global_qps: u32,
    /// 单路由 QPS 上限
    pub route_qps: u32,
    /// 单 IP QPS 上限
    pub ip_qps: u32,
    /// 被动健康检查器（可选）
    pub health_checker: Option<Arc<PassiveHealthChecker>>,
}

impl TrafficControlAdapter {
    /// 使用默认 QPS 值创建（向后兼容）
    pub fn new(
        limiter: Arc<dyn conrogate_contract::traffic::Limiter>,
        breaker_factory: Arc<dyn conrogate_contract::traffic::BreakerFactory>,
    ) -> Self {
        Self {
            limiter,
            breaker_factory,
            global_qps: 1000,
            route_qps: 200,
            ip_qps: 100,
            health_checker: None,
        }
    }

    /// 从限流配置创建
    pub fn with_rate_limit_config(
        limiter: Arc<dyn conrogate_contract::traffic::Limiter>,
        breaker_factory: Arc<dyn conrogate_contract::traffic::BreakerFactory>,
        config: &conrogate_contract::config::RateLimitConfig,
    ) -> Self {
        Self {
            limiter,
            breaker_factory,
            global_qps: config.global_qps,
            route_qps: config.route_qps,
            ip_qps: config.ip_qps,
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
impl conrogate_contract::gateway::TrafficControl for TrafficControlAdapter {
    async fn check_rate_limit(
        &self,
        route_id: u64,
        client_ip: &str,
    ) -> Result<(), ConrogateError> {
        let window = std::time::Duration::from_secs(1);

        // 1. 全局 QPS 限流
        let global_key = "rate:global".to_string();
        self.limiter.acquire(&global_key, self.global_qps, window).await?;

        // 2. 单路由 QPS 限流
        let route_key = format!("rate:route:{route_id}");
        self.limiter.acquire(&route_key, self.route_qps, window).await?;

        // 3. 单路由+单 IP QPS 限流
        let ip_key = format!("rate:{route_id}:{client_ip}");
        self.limiter.acquire(&ip_key, self.ip_qps, window).await
    }

    async fn check_circuit_breaker(
        &self,
        route_id: u64,
        upstream_id: u64,
    ) -> Result<(), ConrogateError> {
        let breaker = self
            .breaker_factory
            .get_or_create(route_id, upstream_id)
            .await;
        breaker.allow().await
    }

    async fn record_result(
        &self,
        route_id: u64,
        upstream_id: u64,
        node_id: u64,
        success: bool,
    ) {
        let breaker = self
            .breaker_factory
            .get_or_create(route_id, upstream_id)
            .await;
        if success {
            breaker.record_success().await;
        } else {
            breaker.record_failure().await;
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
