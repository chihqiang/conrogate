//! HTTP Handler：REST API 端点。
//! 所有 handler 返回统一响应结构 {"code", "msg", "data", "trace_id"}。
//! 写操作（POST/PUT/PATCH/DELETE）要求 Operator 权限。

use super::service::ControlService;
use crate::contract::dto::*;
use crate::contract::response;
use crate::contract::ConrogateError;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use std::sync::Arc;

use super::auth::Role;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub svc: Arc<ControlService>,
    /// API 路由前缀（受保护路由挂载点，用于 OpenAPI 文档生成）
    pub api_prefix: String,
}

/// RBAC 权限校验：不满足返回 Forbidden
fn require_role(role: &Role, required: Role) -> Result<(), ConrogateError> {
    if role.has_permission(required) {
        Ok(())
    } else {
        Err(ConrogateError::Forbidden)
    }
}

// ── 路由管理 ──

pub async fn create_route(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Json(dto): Json<CreateRouteDto>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state.svc.create_route(dto, Some(&operator)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn update_route(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Json(dto): Json<UpdateRouteDto>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state.svc.update_route(dto, Some(&operator)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

/// PATCH 局部更新路由：从路径取 id，body 中字段可选
pub async fn patch_route(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(mut dto): Json<UpdateRouteDto>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    dto.id = id;
    match state.svc.update_route(dto, Some(&operator)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn delete_route(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state.svc.delete_route(id, Some(&operator)).await {
        Ok(_) => response::ok_empty(),
        Err(e) => response::err(e),
    }
}

pub async fn get_route(State(state): State<AppState>, Path(id): Path<u64>) -> Response {
    match state.svc.get_route(id).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

#[derive(serde::Deserialize)]
pub struct PaginationQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

pub async fn list_routes(
    State(state): State<AppState>,
    Query(q): Query<PaginationQuery>,
) -> Response {
    match state
        .svc
        .list_routes(q.page.unwrap_or(1), q.page_size.unwrap_or(20))
        .await
    {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

// ── 上游管理 ──

pub async fn create_upstream(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Json(dto): Json<CreateUpstreamDto>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state.svc.create_upstream(dto, Some(&operator)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn update_upstream(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Json(dto): Json<UpdateUpstreamDto>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state.svc.update_upstream(dto, Some(&operator)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn delete_upstream(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state.svc.delete_upstream(id, Some(&operator)).await {
        Ok(_) => response::ok_empty(),
        Err(e) => response::err(e),
    }
}

/// PATCH 局部更新上游：从路径取 id，body 中字段可选
pub async fn patch_upstream(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(mut dto): Json<UpdateUpstreamDto>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    dto.id = id;
    match state.svc.update_upstream(dto, Some(&operator)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn get_upstream(State(state): State<AppState>, Path(id): Path<u64>) -> Response {
    match state.svc.get_upstream(id).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn list_upstreams(
    State(state): State<AppState>,
    Query(q): Query<PaginationQuery>,
) -> Response {
    match state
        .svc
        .list_upstreams(q.page.unwrap_or(1), q.page_size.unwrap_or(20))
        .await
    {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

// ── 插件绑定 ──

pub async fn bind_plugin(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path(route_id): Path<u64>,
    Json(dto): Json<BindPluginDto>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state.svc.bind_plugin(route_id, dto, Some(&operator)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

/// PUT 更新插件绑定配置
pub async fn update_plugin_binding(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path((route_id, plugin_name)): Path<(u64, String)>,
    Json(dto): Json<UpdatePluginBindingDto>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state
        .svc
        .update_plugin_binding(route_id, &plugin_name, dto, Some(&operator))
        .await
    {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn unbind_plugin(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path((route_id, plugin_name)): Path<(u64, String)>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state
        .svc
        .unbind_plugin(route_id, &plugin_name, Some(&operator))
        .await
    {
        Ok(_) => response::ok_empty(),
        Err(e) => response::err(e),
    }
}

pub async fn list_plugin_bindings(
    State(state): State<AppState>,
    Path(route_id): Path<u64>,
) -> Response {
    match state.svc.list_plugin_bindings(route_id).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

// ── 配置版本 ──

#[derive(serde::Deserialize)]
pub struct PublishQuery {
    pub base_version: Option<u64>,
    pub remark: Option<String>,
}

pub async fn publish_config(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Query(q): Query<PublishQuery>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state
        .svc
        .publish_config(
            q.base_version.unwrap_or(0),
            Some(&operator),
            q.remark.as_deref(),
        )
        .await
    {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn rollback_config(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path(version): Path<u64>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state.svc.rollback_config(version, Some(&operator)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn list_config_versions(
    State(state): State<AppState>,
    Query(q): Query<PaginationQuery>,
) -> Response {
    match state
        .svc
        .list_config_versions(q.page.unwrap_or(1), q.page_size.unwrap_or(20))
        .await
    {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

#[derive(serde::Deserialize)]
pub struct DiffQuery {
    pub from: u64,
    pub to: u64,
}

pub async fn diff_config(State(state): State<AppState>, Query(q): Query<DiffQuery>) -> Response {
    match state.svc.diff_config(q.from, q.to).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

// ── 指标 ──

pub async fn query_metrics(
    State(state): State<AppState>,
    Query(filter): Query<MetricQuery>,
) -> Response {
    match state.svc.query_metrics(filter).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

#[derive(serde::Deserialize)]
pub struct OverviewQuery {
    pub range_min: Option<u32>,
}

pub async fn overview_metrics(
    State(state): State<AppState>,
    Query(q): Query<OverviewQuery>,
) -> Response {
    match state.svc.overview_metrics(q.range_min.unwrap_or(5)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

// ── Insights 聚合查询 ──

pub async fn insights_overview(
    State(state): State<AppState>,
    Query(q): Query<OverviewQuery>,
) -> Response {
    match state.svc.overview_metrics(q.range_min.unwrap_or(5)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn insights_qps(
    State(state): State<AppState>,
    Query(q): Query<OverviewQuery>,
) -> Response {
    match state.svc.insights_qps(q.range_min.unwrap_or(5)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn insights_latency(
    State(state): State<AppState>,
    Query(q): Query<OverviewQuery>,
) -> Response {
    match state.svc.insights_latency(q.range_min.unwrap_or(5)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn insights_status_codes(
    State(state): State<AppState>,
    Query(q): Query<OverviewQuery>,
) -> Response {
    match state
        .svc
        .insights_status_codes(q.range_min.unwrap_or(5))
        .await
    {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

pub async fn insights_top_routes(
    State(state): State<AppState>,
    Query(q): Query<OverviewQuery>,
) -> Response {
    match state
        .svc
        .insights_top_routes(q.range_min.unwrap_or(5))
        .await
    {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

// ── 事件 ──

pub async fn query_events(
    State(state): State<AppState>,
    Query(filter): Query<EventQuery>,
    Query(page): Query<PaginationQuery>,
) -> Response {
    match state
        .svc
        .query_events(filter, page.page.unwrap_or(1), page.page_size.unwrap_or(20))
        .await
    {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

// ── 审计 ──

pub async fn query_audit_logs(
    State(state): State<AppState>,
    Query(filter): Query<AuditLogQuery>,
    Query(page): Query<PaginationQuery>,
) -> Response {
    match state
        .svc
        .query_audit_logs(filter, page.page.unwrap_or(1), page.page_size.unwrap_or(20))
        .await
    {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

// ── 节点 ──

pub async fn list_nodes(State(state): State<AppState>) -> Response {
    match state.svc.list_nodes().await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

// ── 数据上报 ──

pub async fn receive_heartbeat(
    State(state): State<AppState>,
    Json(hb): Json<Heartbeat>,
) -> Response {
    match state.svc.receive_heartbeat(hb).await {
        Ok(_) => response::ok_empty(),
        Err(e) => response::err(e),
    }
}

pub async fn receive_metrics(
    State(state): State<AppState>,
    Json(batch): Json<MetricsBatch>,
) -> Response {
    match state.svc.receive_metrics(batch).await {
        Ok(_) => response::ok_empty(),
        Err(e) => response::err(e),
    }
}

pub async fn receive_events(
    State(state): State<AppState>,
    Json(batch): Json<EventsBatch>,
) -> Response {
    match state.svc.receive_events(batch).await {
        Ok(_) => response::ok_empty(),
        Err(e) => response::err(e),
    }
}

// ── 插件管理（Admin 专属）──

/// 查询已安装插件（所有角色可查）
pub async fn list_plugins(
    State(state): State<AppState>,
    Query(q): Query<PluginStatusQuery>,
) -> Response {
    let status = q
        .status
        .as_deref()
        .and_then(|s| match s.to_lowercase().as_str() {
            "installed" => Some(crate::contract::plugin::PluginStatus::Installed),
            "active" => Some(crate::contract::plugin::PluginStatus::Active),
            "disabled" => Some(crate::contract::plugin::PluginStatus::Disabled),
            "uninstalled" => Some(crate::contract::plugin::PluginStatus::Uninstalled),
            _ => None,
        });
    match state.svc.list_plugins(status).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

#[derive(serde::Deserialize)]
pub struct PluginStatusQuery {
    pub status: Option<String>,
}

/// 激活插件（Admin 专属）
pub async fn activate_plugin(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Admin) {
        return response::err(e);
    }
    match state
        .svc
        .update_plugin_status(
            &name,
            crate::contract::plugin::PluginStatus::Active,
            Some(&operator),
        )
        .await
    {
        Ok(_) => response::ok_empty(),
        Err(e) => response::err(e),
    }
}

/// 禁用插件（Admin 专属）
pub async fn disable_plugin(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Admin) {
        return response::err(e);
    }
    match state
        .svc
        .update_plugin_status(
            &name,
            crate::contract::plugin::PluginStatus::Disabled,
            Some(&operator),
        )
        .await
    {
        Ok(_) => response::ok_empty(),
        Err(e) => response::err(e),
    }
}

/// 卸载插件（Admin 专属）
pub async fn delete_plugin(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Admin) {
        return response::err(e);
    }
    match state.svc.delete_plugin(&name, Some(&operator)).await {
        Ok(_) => response::ok_empty(),
        Err(e) => response::err(e),
    }
}

// ── 全局 IP 黑名单 ──

pub async fn list_ip_blacklist(
    Extension(role): Extension<Role>,
    State(state): State<AppState>,
    Query(q): Query<PaginationQuery>,
    Query(filter): Query<IpBlacklistQuery>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Viewer) {
        return response::err(e);
    }
    match state
        .svc
        .list_ip_blacklist(
            filter,
            q.page.unwrap_or(1),
            q.page_size
                .unwrap_or(crate::contract::constant::DEFAULT_PAGE_SIZE),
        )
        .await
    {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

/// 拉黑 IP/CIDR（Operator 及以上）
pub async fn create_ip_blacklist(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Json(dto): Json<CreateIpBlacklistDto>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state.svc.create_ip_blacklist(dto, Some(&operator)).await {
        Ok(data) => response::ok(data),
        Err(e) => response::err(e),
    }
}

/// 解除拉黑（Operator 及以上）
pub async fn delete_ip_blacklist(
    Extension(role): Extension<Role>,
    Extension(operator): Extension<String>,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    if let Err(e) = require_role(&role, Role::Operator) {
        return response::err(e);
    }
    match state.svc.delete_ip_blacklist(id, Some(&operator)).await {
        Ok(_) => response::ok_empty(),
        Err(e) => response::err(e),
    }
}

// ── 健康检查 ──

pub async fn health_check() -> Response {
    response::ok(serde_json::json!({"status": "ok"}))
}

pub async fn healthz() -> Response {
    response::ok(serde_json::json!({"status": "ok"}))
}

pub async fn readyz(State(state): State<AppState>) -> Response {
    // 1. 检查 DB 连通性
    if let Err(e) = state.svc.list_nodes().await {
        let body = response::error_body(ConrogateError::ERR_CONFIG_LOAD, format!("not ready: {e}"));
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response();
    }
    // 2. 检查路由是否已加载
    match state.svc.list_routes(1, 1).await {
        Ok(routes) if routes.total > 0 => response::ok(serde_json::json!({"status": "ok"})),
        Ok(_) => {
            let body = response::error_body(
                ConrogateError::ERR_CONFIG_LOAD,
                "not ready: no routes loaded",
            );
            (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response()
        }
        Err(e) => {
            let body = response::error_body(
                ConrogateError::ERR_CONFIG_LOAD,
                format!("not ready: route check failed: {e}"),
            );
            (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response()
        }
    }
}
