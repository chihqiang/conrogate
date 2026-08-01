//! DTO 定义。

use crate::balancer::BalancerAlgorithm;
use crate::protocol::{ProtocolId, RouteMatchConditions};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── 路由 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDto {
    pub id: u64,
    pub name: String,
    pub protocol: ProtocolId,
    pub match_conditions: RouteMatchConditions,
    pub priority: i32,
    pub upstream_id: Option<u64>,
    pub host_header: Option<String>,
    pub allow_retry_non_idempotent: bool,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRouteDto {
    pub name: String,
    pub protocol: ProtocolId,
    pub match_conditions: RouteMatchConditions,
    pub priority: Option<i32>,
    pub upstream_id: Option<u64>,
    pub host_header: Option<String>,
    pub allow_retry_non_idempotent: Option<bool>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRouteDto {
    pub id: u64,
    pub name: Option<String>,
    pub match_conditions: Option<RouteMatchConditions>,
    pub priority: Option<i32>,
    pub upstream_id: Option<u64>,
    pub host_header: Option<String>,
    pub allow_retry_non_idempotent: Option<bool>,
    pub enabled: Option<bool>,
}

// ── 上游 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamNodeDto {
    pub id: u64,
    pub upstream_id: u64,
    pub address: String,
    pub weight: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamDto {
    pub id: u64,
    pub name: String,
    pub algorithm: BalancerAlgorithm,
    pub retry_enabled: bool,
    pub nodes: Vec<UpstreamNodeDto>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUpstreamDto {
    pub name: String,
    pub algorithm: BalancerAlgorithm,
    pub retry_enabled: Option<bool>,
    pub nodes: Vec<CreateUpstreamNodeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUpstreamNodeDto {
    pub address: String,
    pub weight: Option<i32>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUpstreamDto {
    pub id: u64,
    pub name: Option<String>,
    pub algorithm: Option<BalancerAlgorithm>,
    pub retry_enabled: Option<bool>,
    pub nodes: Option<Vec<CreateUpstreamNodeDto>>,
}

// ── 插件绑定 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginBindingDto {
    pub id: u64,
    pub route_id: u64,
    pub plugin_name: String,
    pub config: serde_json::Value,
    pub order: i32,
    pub blocking: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindPluginDto {
    pub plugin_name: String,
    pub config: serde_json::Value,
    pub order: Option<i32>,
    pub blocking: Option<bool>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePluginBindingDto {
    pub config: Option<serde_json::Value>,
    pub order: Option<i32>,
    pub blocking: Option<bool>,
    pub enabled: Option<bool>,
}

// ── 配置版本 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishType {
    Publish,
    Rollback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub routes: Vec<RouteDto>,
    pub upstreams: Vec<UpstreamDto>,
    pub plugin_bindings: Vec<PluginBindingDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDiff {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
}

// ── 指标与事件 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricRow {
    pub ts: DateTime<Utc>,
    pub bucket_sec: u32,
    pub route_id: Option<u64>,
    pub gate_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub ts: DateTime<Utc>,
    pub event_type: String,
    pub route_id: Option<u64>,
    pub upstream_id: Option<u64>,
    pub trace_id: Option<String>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewMetric {
    pub total_qps: f64,
    pub avg_latency_ms: f64,
    pub error_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult<T> {
    pub list: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

// ── 审计日志 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeApplicationRow {
    pub gate_id: String,
    pub version: u64,
    pub applied_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── 已安装插件 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginDto {
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub kind: crate::plugin::PluginKind,
    pub status: crate::plugin::PluginStatus,
    pub package_hash: Option<String>,
    pub manifest: serde_json::Value,
    pub installed_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
}

// ── 数据上报载荷 ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsBatch {
    pub gate_id: String,
    pub trace_id: String,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub bucket_sec: u32,
    pub metrics: Vec<MetricRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventsBatch {
    pub gate_id: String,
    pub trace_id: String,
    pub events: Vec<EventRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub host_header: Option<String>,
    pub allow_retry_non_idempotent: bool,
    pub plugin_chain: Vec<PluginBindingDto>,
}

// ── 查询参数 ──

#[derive(Debug, Clone, Deserialize)]
pub struct MetricQuery {
    pub range_min: u32,
    pub route_id: Option<u64>,
    pub gate_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EventQuery {
    pub event_type: Option<String>,
    pub route_id: Option<u64>,
    pub ts_from: Option<DateTime<Utc>>,
    pub ts_to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuditLogQuery {
    pub operator: Option<String>,
    pub action: Option<String>,
    pub resource: Option<String>,
    pub ts_from: Option<DateTime<Utc>>,
    pub ts_to: Option<DateTime<Utc>>,
}
