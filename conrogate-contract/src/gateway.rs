//! 网关核心 Trait：协议适配层、ServiceContext、仓储层、插件注册与调度。

use crate::dto::{EventRow, MetricRow, RouteSnapshot, UpstreamNodeDto};
use crate::error::ConrogateError;
use crate::plugin::{Plugin, PluginContext, PluginOutcome, PluginResponse};
use crate::protocol::{ProtocolId, RouteMatchInfo};
use async_trait::async_trait;
use std::sync::Arc;

// ── 路由查询 ──

#[async_trait]
pub trait RouteLookup: Send + Sync {
    async fn lookup_route(
        &self,
        protocol: ProtocolId,
        info: &RouteMatchInfo,
    ) -> Result<Option<RouteSnapshot>, ConrogateError>;
}

// ── 上游选择 ──

#[async_trait]
pub trait UpstreamSelector: Send + Sync {
    async fn select_upstream(
        &self,
        route: &RouteSnapshot,
    ) -> Result<UpstreamNodeDto, ConrogateError>;
}

// ── 流量治理 ──

#[async_trait]
pub trait TrafficControl: Send + Sync {
    async fn check_rate_limit(
        &self,
        route_id: u64,
        client_ip: &str,
    ) -> Result<(), ConrogateError>;

    async fn check_circuit_breaker(
        &self,
        route_id: u64,
        upstream_id: u64,
    ) -> Result<(), ConrogateError>;

    async fn record_result(
        &self,
        route_id: u64,
        upstream_id: u64,
        node_id: u64,
        success: bool,
    );
}

// ── 遥测上报 ──

#[async_trait]
pub trait TelemetryReport: Send + Sync {
    async fn record_metric(&self, metric: MetricRow);
    async fn record_event(&self, event: EventRow);
}

// ── 插件执行 ──

#[async_trait]
pub trait PluginExecutor: Send + Sync {
    async fn execute_before_request(
        &self,
        ctx: &mut PluginContext,
        plugins: &[Arc<dyn Plugin>],
    ) -> Result<PluginOutcome, ConrogateError>;

    async fn execute_after_response(
        &self,
        ctx: &mut PluginContext,
        resp: &mut PluginResponse,
        plugins: &[Arc<dyn Plugin>],
    ) -> Result<(), ConrogateError>;

    async fn execute_on_connect(
        &self,
        ctx: &mut PluginContext,
        plugins: &[Arc<dyn Plugin>],
    ) -> Result<PluginOutcome, ConrogateError>;

    async fn execute_on_disconnect(
        &self,
        ctx: &mut PluginContext,
        plugins: &[Arc<dyn Plugin>],
    ) -> Result<(), ConrogateError>;
}

/// ServiceContext 聚合所有能力，供 ProtocolHandler 使用
pub struct ServiceContext {
    pub routes: Arc<dyn RouteLookup>,
    pub balancer: Arc<dyn UpstreamSelector>,
    pub traffic: Arc<dyn TrafficControl>,
    pub telemetry: Arc<dyn TelemetryReport>,
    pub plugins: Arc<dyn PluginExecutor>,
}

impl std::fmt::Debug for ServiceContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceContext").finish()
    }
}
