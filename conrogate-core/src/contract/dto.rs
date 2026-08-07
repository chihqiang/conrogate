//! DTO 定义。

use crate::contract::balancer::BalancerAlgorithm;
use crate::contract::protocol::{ProtocolId, RouteMatchConditions};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── 路由 ──

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RouteDto {
    pub id: u64,
    pub name: String,
    pub protocol: ProtocolId,
    pub match_conditions: RouteMatchConditions,
    pub priority: i32,
    pub upstream_id: Option<u64>,
    pub host_header: Option<String>,
    pub allow_retry_non_idempotent: bool,
    /// WS 隧道转发上游时是否剥离敏感头（authorization/cookie/x-api-key 等）。
    /// 默认 false（透传，保留当前行为）；启用后与 HTTP 转发路径的安全模型一致。
    #[serde(default)]
    pub ws_strip_sensitive_headers: bool,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateRouteDto {
    pub name: String,
    pub protocol: ProtocolId,
    pub match_conditions: RouteMatchConditions,
    pub priority: Option<i32>,
    pub upstream_id: Option<u64>,
    pub host_header: Option<String>,
    pub allow_retry_non_idempotent: Option<bool>,
    #[serde(default)]
    pub ws_strip_sensitive_headers: Option<bool>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateRouteDto {
    /// PATCH 时从路径取值覆盖，body 可省略；PUT 时必须携带
    #[serde(default)]
    pub id: u64,
    pub name: Option<String>,
    pub match_conditions: Option<RouteMatchConditions>,
    pub priority: Option<i32>,
    pub upstream_id: Option<u64>,
    pub host_header: Option<String>,
    pub allow_retry_non_idempotent: Option<bool>,
    #[serde(default)]
    pub ws_strip_sensitive_headers: Option<bool>,
    pub enabled: Option<bool>,
}

// ── 上游 ──

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpstreamNodeDto {
    pub id: u64,
    pub upstream_id: u64,
    pub address: String,
    pub weight: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpstreamDto {
    pub id: u64,
    pub name: String,
    pub algorithm: BalancerAlgorithm,
    pub retry_enabled: bool,
    pub nodes: Vec<UpstreamNodeDto>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateUpstreamDto {
    pub name: String,
    pub algorithm: BalancerAlgorithm,
    pub retry_enabled: Option<bool>,
    pub nodes: Vec<CreateUpstreamNodeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateUpstreamNodeDto {
    pub address: String,
    pub weight: Option<i32>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateUpstreamDto {
    /// PATCH 时从路径取值覆盖，body 可省略；PUT 时必须携带
    #[serde(default)]
    pub id: u64,
    pub name: Option<String>,
    pub algorithm: Option<BalancerAlgorithm>,
    pub retry_enabled: Option<bool>,
    pub nodes: Option<Vec<CreateUpstreamNodeDto>>,
}

// ── 插件绑定 ──

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PluginBindingDto {
    pub id: u64,
    pub route_id: u64,
    pub plugin_name: String,
    pub config: serde_json::Value,
    pub order: i32,
    pub blocking: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BindPluginDto {
    pub plugin_name: String,
    pub config: serde_json::Value,
    pub order: Option<i32>,
    pub blocking: Option<bool>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdatePluginBindingDto {
    pub config: Option<serde_json::Value>,
    pub order: Option<i32>,
    pub blocking: Option<bool>,
    pub enabled: Option<bool>,
}

// ── 配置版本 ──

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConfigVersionDto {
    pub version: u64,
    pub base_version: u64,
    pub publish_type: PublishType,
    pub content_hash: String,
    pub created_by: Option<String>,
    pub remark: Option<String>,
    pub applied_count: u32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublishType {
    Publish,
    Rollback,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConfigSnapshot {
    pub routes: Vec<RouteDto>,
    pub upstreams: Vec<UpstreamDto>,
    pub plugin_bindings: Vec<PluginBindingDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConfigDiff {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
}

// ── 指标与事件 ──

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MetricRow {
    pub ts: DateTime<Utc>,
    /// 时间桶长度（秒）。协议层原始样本填 0，由 MetricAggregator 按自身配置重写
    pub bucket_sec: u32,
    pub route_id: Option<u64>,
    /// 网关实例标识（多网关部署区分数据来源）
    pub gate_id: String,
    /// 协议层原始样本填 0，由 MetricAggregator 聚合时重算
    pub qps: u32,
    pub total_requests: u64,
    pub avg_latency_ms: f64,
    pub p50_ms: u32,
    pub p90_ms: u32,
    pub p99_ms: u32,
    pub status_2xx: u64,
    pub status_3xx: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,
    pub sessions: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

impl MetricRow {
    /// 构建协议层单次请求/会话的原始样本。
    ///
    /// `total_requests` 恒为 1；`bucket_sec`/`qps` 为占位值，
    /// 由 `MetricAggregator` 按自身桶配置聚合时重写（见 `crate::gateway::telemetry`）。
    #[allow(clippy::too_many_arguments)]
    pub fn raw_sample(
        ts: DateTime<Utc>,
        gate_id: String,
        route_id: Option<u64>,
        latency_ms: f64,
        p50_ms: u32,
        p90_ms: u32,
        p99_ms: u32,
        status_2xx: u64,
        status_3xx: u64,
        status_4xx: u64,
        status_5xx: u64,
        sessions: u64,
        bytes_in: u64,
        bytes_out: u64,
    ) -> Self {
        Self {
            ts,
            bucket_sec: 0,
            route_id,
            gate_id,
            qps: 0,
            total_requests: 1,
            avg_latency_ms: latency_ms,
            p50_ms,
            p90_ms,
            p99_ms,
            status_2xx,
            status_3xx,
            status_4xx,
            status_5xx,
            sessions,
            bytes_in,
            bytes_out,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EventRow {
    pub ts: DateTime<Utc>,
    pub event_type: String,
    pub route_id: Option<u64>,
    pub upstream_id: Option<u64>,
    pub trace_id: Option<String>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OverviewMetric {
    pub total_qps: f64,
    pub avg_latency_ms: f64,
    pub error_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PaginatedResult<T> {
    pub list: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

// ── 审计日志 ──

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AuditLogRow {
    pub ts: DateTime<Utc>,
    pub operator: Option<String>,
    pub action: String,
    pub resource: String,
    pub resource_id: Option<u64>,
    pub detail: serde_json::Value,
    pub trace_id: Option<String>,
}

// ── 节点应用记录 ──

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NodeApplicationRow {
    pub gate_id: String,
    pub version: u64,
    pub applied_at: DateTime<Utc>,
    /// 最近心跳时间
    pub last_seen: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── 已安装插件 ──

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InstalledPluginDto {
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub kind: crate::contract::plugin::PluginKind,
    pub status: crate::contract::plugin::PluginStatus,
    pub package_hash: Option<String>,
    pub manifest: serde_json::Value,
    pub installed_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
}

// ── 数据上报载荷 ──

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MetricsBatch {
    pub gate_id: String,
    pub trace_id: String,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub bucket_sec: u32,
    pub metrics: Vec<MetricRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EventsBatch {
    pub gate_id: String,
    pub trace_id: String,
    pub events: Vec<EventRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Heartbeat {
    pub gate_id: String,
    pub version: u64,
    pub timestamp: DateTime<Utc>,
}

/// 路由快照（内存中的路由完整信息）
#[derive(Debug, Clone)]
pub struct RouteSnapshot {
    pub id: u64,
    pub protocol: ProtocolId,
    pub upstream_id: Option<u64>,
    /// 上游 Host 头（Arc<str> 共享：请求热路径避免每次匹配克隆 String）
    pub host_header: Option<Arc<str>>,
    pub allow_retry_non_idempotent: bool,
    /// WS 隧道转发上游时是否剥离敏感头（与 HTTP 路径安全模型一致）
    pub ws_strip_sensitive_headers: bool,
    /// 路由绑定的插件链（Arc 共享：请求热路径避免整份 Vec 克隆，配置热加载时整体替换）
    pub plugin_chain: Arc<Vec<PluginBindingDto>>,
    /// 该路由是否有 requires_body 插件 → true 时网关以缓冲模式处理请求体
    pub requires_body: bool,
}

// ── 查询参数 ──

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct MetricQuery {
    pub range_min: u32,
    pub route_id: Option<u64>,
    pub gate_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct EventQuery {
    pub event_type: Option<String>,
    pub route_id: Option<u64>,
    pub ts_from: Option<DateTime<Utc>>,
    pub ts_to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct AuditLogQuery {
    pub operator: Option<String>,
    pub action: Option<String>,
    pub resource: Option<String>,
    pub ts_from: Option<DateTime<Utc>>,
    pub ts_to: Option<DateTime<Utc>>,
}
