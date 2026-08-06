//! 网关核心 Trait：协议适配层、ServiceContext、仓储层、插件注册与调度。

use crate::contract::dto::{EventRow, MetricRow, RouteSnapshot, UpstreamNodeDto};
use crate::contract::error::ConrogateError;
use crate::contract::plugin::{Plugin, PluginContext, PluginOutcome, PluginResponse};
use crate::contract::protocol::{ProtocolId, RouteMatchInfo};
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
    /// 选择上游节点。`key` 用于有状态/哈希算法（一致性哈希按 client_ip）。
    async fn select_upstream(
        &self,
        route: &RouteSnapshot,
        key: Option<&str>,
    ) -> Result<UpstreamNodeDto, ConrogateError>;

    /// 释放节点（请求/连接结束时回调，供 LeastConnections 等有状态算法递减计数）
    async fn release_node(&self, _route: &RouteSnapshot, _node: &UpstreamNodeDto) {}
}

// ── 流量治理 ──

#[async_trait]
pub trait TrafficControl: Send + Sync {
    async fn check_rate_limit(&self, route_id: u64, client_ip: &str) -> Result<(), ConrogateError>;

    async fn check_circuit_breaker(
        &self,
        route_id: u64,
        node_id: u64,
    ) -> Result<(), ConrogateError>;

    async fn record_result(&self, route_id: u64, node_id: u64, success: bool);
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
    /// 获取路由当前生效的插件链（配置热加载后原子替换；每绑定独立实例）
    fn route_plugins(&self, route_id: u64) -> Vec<Arc<dyn Plugin>>;

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
    /// 网关实例标识：写入遥测指标，用于多网关部署时区分数据来源
    pub gate_id: String,
}

impl std::fmt::Debug for ServiceContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceContext").finish()
    }
}
