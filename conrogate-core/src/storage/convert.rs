//! Entity ↔ DTO 双向转换。

use crate::contract::balancer::BalancerAlgorithm;
use crate::contract::dto::*;
use crate::contract::plugin::{PluginKind, PluginStatus};
use crate::contract::protocol::{ProtocolId, RouteMatchConditions};
use crate::storage::entity::{
    audit_logs, config_versions, gateway_events, installed_plugins, metric_aggregates,
    node_applications, route_plugin_bindings, routes, upstream_nodes, upstreams,
};
use sea_orm::Set;
use serde_json;

// ── 协议枚举映射 ──

// protocol 编号 1=http 2=websocket 3=tcp_tunnel
pub(crate) fn protocol_to_i16(p: ProtocolId) -> i16 {
    match p {
        ProtocolId::Http => 1,
        ProtocolId::WebSocket => 2,
        ProtocolId::TcpTunnel => 3,
    }
}

fn i16_to_protocol(v: i16) -> ProtocolId {
    match v {
        2 => ProtocolId::WebSocket,
        3 => ProtocolId::TcpTunnel,
        _ => ProtocolId::Http,
    }
}

// ── 算法枚举映射 ──

// algorithm 编号 1=round_robin 2=weighted_round_robin 3=least_connections 4=consistent_hash
pub(crate) fn algorithm_to_i16(a: BalancerAlgorithm) -> i16 {
    match a {
        BalancerAlgorithm::RoundRobin => 1,
        BalancerAlgorithm::WeightedRoundRobin => 2,
        BalancerAlgorithm::LeastConnections => 3,
        BalancerAlgorithm::ConsistentHash => 4,
    }
}

fn i16_to_algorithm(v: i16) -> BalancerAlgorithm {
    match v {
        2 => BalancerAlgorithm::WeightedRoundRobin,
        3 => BalancerAlgorithm::LeastConnections,
        4 => BalancerAlgorithm::ConsistentHash,
        _ => BalancerAlgorithm::RoundRobin,
    }
}

// ── PublishType 映射 ──

fn i16_to_publish_type(v: i16) -> PublishType {
    match v {
        1 => PublishType::Rollback,
        _ => PublishType::Publish,
    }
}

// ── PluginKind / PluginStatus 映射 ──

fn plugin_kind_to_i16(k: PluginKind) -> i16 {
    match k {
        PluginKind::Native => 0,
        PluginKind::Wasm => 1,
    }
}

fn i16_to_plugin_kind(v: i16) -> PluginKind {
    match v {
        1 => PluginKind::Wasm,
        _ => PluginKind::Native,
    }
}

fn plugin_status_to_i16(s: PluginStatus) -> i16 {
    match s {
        PluginStatus::Installed => 0,
        PluginStatus::Active => 1,
        PluginStatus::Disabled => 2,
        PluginStatus::Uninstalled => 3,
    }
}

fn i16_to_plugin_status(v: i16) -> PluginStatus {
    match v {
        1 => PluginStatus::Active,
        2 => PluginStatus::Disabled,
        3 => PluginStatus::Uninstalled,
        _ => PluginStatus::Installed,
    }
}

// ── routes ──

pub fn route_model_to_dto(m: routes::Model) -> Option<RouteDto> {
    let conditions: RouteMatchConditions = serde_json::from_value(m.match_conditions).ok()?;
    Some(RouteDto {
        id: m.id as u64,
        name: m.name,
        protocol: i16_to_protocol(m.protocol),
        match_conditions: conditions,
        priority: m.priority,
        upstream_id: m.upstream_id.map(|v| v as u64),
        host_header: m.host_header,
        allow_retry_non_idempotent: m.allow_retry_non_idempotent,
        ws_strip_sensitive_headers: m.ws_strip_sensitive_headers,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub fn route_create_to_active_model(dto: CreateRouteDto) -> routes::ActiveModel {
    let conditions_json = serde_json::to_value(&dto.match_conditions).unwrap_or_default();
    routes::ActiveModel {
        name: Set(dto.name),
        protocol: Set(protocol_to_i16(dto.protocol)),
        match_conditions: Set(conditions_json),
        priority: Set(dto.priority.unwrap_or(10)),
        upstream_id: Set(dto.upstream_id.map(|v| v as i64)),
        host_header: Set(dto.host_header),
        allow_retry_non_idempotent: Set(dto.allow_retry_non_idempotent.unwrap_or(false)),
        ws_strip_sensitive_headers: Set(dto.ws_strip_sensitive_headers.unwrap_or(false)),
        enabled: Set(dto.enabled.unwrap_or(true)),
        created_at: Set(chrono::Utc::now()),
        updated_at: Set(chrono::Utc::now()),
        deleted_at: Set(None),
        ..Default::default()
    }
}

// ── upstreams + nodes ──

pub fn upstream_model_to_dto(
    m: upstreams::Model,
    nodes: Vec<upstream_nodes::Model>,
) -> Option<UpstreamDto> {
    Some(UpstreamDto {
        id: m.id as u64,
        name: m.name,
        algorithm: i16_to_algorithm(m.algorithm),
        retry_enabled: m.retry_enabled,
        nodes: nodes
            .into_iter()
            .filter_map(upstream_node_model_to_dto)
            .collect(),
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub fn upstream_node_model_to_dto(m: upstream_nodes::Model) -> Option<UpstreamNodeDto> {
    Some(UpstreamNodeDto {
        id: m.id as u64,
        upstream_id: m.upstream_id as u64,
        address: m.address,
        weight: m.weight,
        enabled: m.enabled,
    })
}

pub fn upstream_create_to_active_model(dto: CreateUpstreamDto) -> upstreams::ActiveModel {
    upstreams::ActiveModel {
        name: Set(dto.name),
        algorithm: Set(algorithm_to_i16(dto.algorithm)),
        retry_enabled: Set(dto.retry_enabled.unwrap_or(true)),
        created_at: Set(chrono::Utc::now()),
        updated_at: Set(chrono::Utc::now()),
        deleted_at: Set(None),
        ..Default::default()
    }
}

pub fn node_create_to_active_model(
    upstream_id: i64,
    dto: CreateUpstreamNodeDto,
) -> upstream_nodes::ActiveModel {
    upstream_nodes::ActiveModel {
        upstream_id: Set(upstream_id),
        address: Set(dto.address),
        weight: Set(dto.weight.unwrap_or(1)),
        enabled: Set(dto.enabled.unwrap_or(true)),
        created_at: Set(chrono::Utc::now()),
        updated_at: Set(chrono::Utc::now()),
        deleted_at: Set(None),
        ..Default::default()
    }
}

// ── route_plugin_bindings ──

pub fn binding_model_to_dto(m: route_plugin_bindings::Model) -> Option<PluginBindingDto> {
    Some(PluginBindingDto {
        id: m.id as u64,
        route_id: m.route_id as u64,
        plugin_name: m.plugin_name,
        config: m.config,
        order: m.order,
        blocking: m.blocking,
        enabled: m.enabled,
    })
}

pub fn binding_create_to_active_model(
    route_id: i64,
    dto: BindPluginDto,
) -> route_plugin_bindings::ActiveModel {
    route_plugin_bindings::ActiveModel {
        route_id: Set(route_id),
        plugin_name: Set(dto.plugin_name),
        config: Set(dto.config),
        order: Set(dto.order.unwrap_or(0)),
        blocking: Set(dto.blocking.unwrap_or(false)),
        enabled: Set(dto.enabled.unwrap_or(true)),
        created_at: Set(chrono::Utc::now()),
        updated_at: Set(chrono::Utc::now()),
        deleted_at: Set(None),
        ..Default::default()
    }
}

// ── config_versions ──

pub fn config_version_model_to_dto(m: config_versions::Model) -> Option<ConfigVersionDto> {
    Some(ConfigVersionDto {
        version: m.version as u64,
        base_version: m.base_version as u64,
        publish_type: i16_to_publish_type(m.publish_type),
        content_hash: m.content_hash,
        created_by: m.created_by,
        remark: m.remark,
        applied_count: 0, // 从 node_applications 查询
        created_at: m.created_at,
    })
}

// ── metric_aggregates ──

pub fn metric_model_to_row(m: metric_aggregates::Model) -> Option<MetricRow> {
    Some(MetricRow {
        ts: m.ts,
        bucket_sec: m.bucket_sec as u32,
        route_id: m.route_id.map(|v| v as u64),
        gate_id: m.gate_id,
        qps: m.qps as u32,
        total_requests: m.total_requests as u64,
        avg_latency_ms: m.avg_latency_ms,
        p50_ms: m.p50_ms as u32,
        p90_ms: m.p90_ms as u32,
        p99_ms: m.p99_ms as u32,
        status_2xx: m.status_2xx as u64,
        status_3xx: m.status_3xx as u64,
        status_4xx: m.status_4xx as u64,
        status_5xx: m.status_5xx as u64,
        sessions: m.sessions as u64,
        bytes_in: m.bytes_in as u64,
        bytes_out: m.bytes_out as u64,
    })
}

pub fn metric_row_to_active_model(row: &MetricRow) -> metric_aggregates::ActiveModel {
    metric_aggregates::ActiveModel {
        ts: Set(row.ts),
        bucket_sec: Set(row.bucket_sec as i32),
        route_id: Set(row.route_id.map(|v| v as i64)),
        gate_id: Set(row.gate_id.clone()),
        qps: Set(row.qps as i32),
        total_requests: Set(row.total_requests as i64),
        avg_latency_ms: Set(row.avg_latency_ms),
        p50_ms: Set(row.p50_ms as i32),
        p90_ms: Set(row.p90_ms as i32),
        p99_ms: Set(row.p99_ms as i32),
        status_2xx: Set(row.status_2xx as i64),
        status_3xx: Set(row.status_3xx as i64),
        status_4xx: Set(row.status_4xx as i64),
        status_5xx: Set(row.status_5xx as i64),
        sessions: Set(row.sessions as i64),
        bytes_in: Set(row.bytes_in as i64),
        bytes_out: Set(row.bytes_out as i64),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    }
}

// ── gateway_events ──

pub fn event_model_to_row(m: gateway_events::Model) -> Option<EventRow> {
    Some(EventRow {
        ts: m.ts,
        event_type: m.event_type,
        route_id: m.route_id.map(|v| v as u64),
        upstream_id: m.upstream_id.map(|v| v as u64),
        trace_id: m.trace_id,
        detail: m.detail.unwrap_or(serde_json::Value::Null),
    })
}

pub fn event_row_to_active_model(row: &EventRow) -> gateway_events::ActiveModel {
    gateway_events::ActiveModel {
        ts: Set(row.ts),
        event_type: Set(row.event_type.clone()),
        route_id: Set(row.route_id.map(|v| v as i64)),
        upstream_id: Set(row.upstream_id.map(|v| v as i64)),
        trace_id: Set(row.trace_id.clone()),
        detail: Set(Some(row.detail.clone())),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    }
}

// ── audit_logs ──

pub fn audit_row_to_active_model(row: &AuditLogRow) -> audit_logs::ActiveModel {
    audit_logs::ActiveModel {
        ts: Set(row.ts),
        operator: Set(row.operator.clone()),
        action: Set(row.action.clone()),
        resource: Set(row.resource.clone()),
        resource_id: Set(row.resource_id.map(|v| v as i64)),
        detail: Set(Some(row.detail.clone())),
        trace_id: Set(row.trace_id.clone()),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    }
}

pub fn audit_model_to_row(m: audit_logs::Model) -> Option<AuditLogRow> {
    Some(AuditLogRow {
        ts: m.ts,
        operator: m.operator,
        action: m.action,
        resource: m.resource,
        resource_id: m.resource_id.map(|v| v as u64),
        detail: m.detail.unwrap_or(serde_json::Value::Null),
        trace_id: m.trace_id,
    })
}

// ── node_applications ──

pub fn node_app_model_to_row(m: node_applications::Model) -> Option<NodeApplicationRow> {
    Some(NodeApplicationRow {
        gate_id: m.gate_id,
        version: m.version as u64,
        applied_at: m.applied_at,
        last_seen: m.last_seen,
        updated_at: m.updated_at,
    })
}

// ── installed_plugins ──

pub fn installed_plugin_model_to_dto(m: installed_plugins::Model) -> Option<InstalledPluginDto> {
    Some(InstalledPluginDto {
        name: m.name,
        version: m.version,
        api_version: m.api_version as u32,
        kind: i16_to_plugin_kind(m.kind),
        status: i16_to_plugin_status(m.status),
        package_hash: m.package_hash,
        manifest: m.manifest,
        installed_at: m.installed_at,
        activated_at: m.activated_at,
    })
}

pub fn installed_plugin_dto_to_active_model(
    dto: &InstalledPluginDto,
) -> installed_plugins::ActiveModel {
    installed_plugins::ActiveModel {
        name: Set(dto.name.clone()),
        version: Set(dto.version.clone()),
        api_version: Set(dto.api_version as i32),
        kind: Set(plugin_kind_to_i16(dto.kind)),
        status: Set(plugin_status_to_i16(dto.status)),
        package_hash: Set(dto.package_hash.clone()),
        manifest: Set(dto.manifest.clone()),
        installed_at: Set(dto.installed_at),
        activated_at: Set(dto.activated_at),
        deleted_at: Set(None),
        ..Default::default()
    }
}
