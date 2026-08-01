//! 请求过滤器：限流拦截 + 熔断检查 + 配置刷新热载。

use conrogate_contract::config::Config;
use conrogate_contract::ConrogateError;
use std::sync::Arc;
use std::sync::RwLock;

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

/// 流量治理适配器：整合限流 + 熔断
pub struct TrafficControlAdapter {
    pub limiter: Arc<dyn conrogate_contract::traffic::Limiter>,
    pub breaker_factory: Arc<dyn conrogate_contract::traffic::BreakerFactory>,
}

#[async_trait::async_trait]
impl conrogate_contract::gateway::TrafficControl for TrafficControlAdapter {
    async fn check_rate_limit(
        &self,
        route_id: u64,
        client_ip: &str,
    ) -> Result<(), ConrogateError> {
        let key = format!("rate:{route_id}:{client_ip}");
        // 默认 100 QPS per route per IP
        self.limiter
            .acquire(&key, 100, std::time::Duration::from_secs(1))
            .await
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
    }
}
