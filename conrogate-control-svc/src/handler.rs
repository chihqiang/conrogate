//! HTTP Handler：REST API 端点。

use crate::service::ControlService;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;
use conrogate_contract::dto::*;
use conrogate_contract::ConrogateError;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub svc: Arc<ControlService>,
}

// ── 路由管理 ──

pub async fn create_route(
    State(state): State<AppState>,
    Json(dto): Json<CreateRouteDto>,
) -> Result<Json<RouteDto>, (StatusCode, String)> {
    state
        .svc
        .create_route(dto, None)
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

pub async fn update_route(
    State(state): State<AppState>,
    Json(dto): Json<UpdateRouteDto>,
) -> Result<Json<RouteDto>, (StatusCode, String)> {
    state
        .svc
        .update_route(dto, None)
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

pub async fn delete_route(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .svc
        .delete_route(id, None)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| to_error(e))
}

pub async fn get_route(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Option<RouteDto>>, (StatusCode, String)> {
    state
        .svc
        .get_route(id)
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

#[derive(serde::Deserialize)]
pub struct PaginationQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

pub async fn list_routes(
    State(state): State<AppState>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<RouteDto>>, (StatusCode, String)> {
    state
        .svc
        .list_routes(q.page.unwrap_or(1), q.page_size.unwrap_or(20))
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

// ── 上游管理 ──

pub async fn create_upstream(
    State(state): State<AppState>,
    Json(dto): Json<CreateUpstreamDto>,
) -> Result<Json<UpstreamDto>, (StatusCode, String)> {
    state
        .svc
        .create_upstream(dto, None)
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

pub async fn update_upstream(
    State(state): State<AppState>,
    Json(dto): Json<UpdateUpstreamDto>,
) -> Result<Json<UpstreamDto>, (StatusCode, String)> {
    state
        .svc
        .update_upstream(dto, None)
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

pub async fn delete_upstream(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .svc
        .delete_upstream(id, None)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| to_error(e))
}

pub async fn get_upstream(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Option<UpstreamDto>>, (StatusCode, String)> {
    state
        .svc
        .get_upstream(id)
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

pub async fn list_upstreams(
    State(state): State<AppState>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<UpstreamDto>>, (StatusCode, String)> {
    state
        .svc
        .list_upstreams(q.page.unwrap_or(1), q.page_size.unwrap_or(20))
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

// ── 插件绑定 ──

pub async fn bind_plugin(
    State(state): State<AppState>,
    Path(route_id): Path<u64>,
    Json(dto): Json<BindPluginDto>,
) -> Result<Json<PluginBindingDto>, (StatusCode, String)> {
    state
        .svc
        .bind_plugin(route_id, dto, None)
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

pub async fn unbind_plugin(
    State(state): State<AppState>,
    Path((route_id, plugin_name)): Path<(u64, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .svc
        .unbind_plugin(route_id, &plugin_name, None)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| to_error(e))
}

pub async fn list_plugin_bindings(
    State(state): State<AppState>,
    Path(route_id): Path<u64>,
) -> Result<Json<Vec<PluginBindingDto>>, (StatusCode, String)> {
    state
        .svc
        .list_plugin_bindings(route_id)
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

// ── 配置版本 ──

#[derive(serde::Deserialize)]
pub struct PublishQuery {
    pub base_version: Option<u64>,
    pub remark: Option<String>,
}

pub async fn publish_config(
    State(state): State<AppState>,
    Query(q): Query<PublishQuery>,
) -> Result<Json<ConfigVersionDto>, (StatusCode, String)> {
    state
        .svc
        .publish_config(q.base_version.unwrap_or(0), None, q.remark.as_deref())
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

#[derive(serde::Deserialize)]
pub struct RollbackQuery {
    pub target_version: u64,
}

pub async fn rollback_config(
    State(state): State<AppState>,
    Query(q): Query<RollbackQuery>,
) -> Result<Json<ConfigVersionDto>, (StatusCode, String)> {
    state
        .svc
        .rollback_config(q.target_version, None)
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

pub async fn list_config_versions(
    State(state): State<AppState>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<ConfigVersionDto>>, (StatusCode, String)> {
    state
        .svc
        .list_config_versions(q.page.unwrap_or(1), q.page_size.unwrap_or(20))
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

#[derive(serde::Deserialize)]
pub struct DiffQuery {
    pub from: u64,
    pub to: u64,
}

pub async fn diff_config(
    State(state): State<AppState>,
    Query(q): Query<DiffQuery>,
) -> Result<Json<ConfigDiff>, (StatusCode, String)> {
    state
        .svc
        .diff_config(q.from, q.to)
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

// ── 指标 ──

pub async fn query_metrics(
    State(state): State<AppState>,
    Query(filter): Query<MetricQuery>,
) -> Result<Json<Vec<MetricRow>>, (StatusCode, String)> {
    state
        .svc
        .query_metrics(filter)
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

#[derive(serde::Deserialize)]
pub struct OverviewQuery {
    pub range_min: Option<u32>,
}

pub async fn overview_metrics(
    State(state): State<AppState>,
    Query(q): Query<OverviewQuery>,
) -> Result<Json<OverviewMetric>, (StatusCode, String)> {
    state
        .svc
        .overview_metrics(q.range_min.unwrap_or(5))
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

// ── 事件 ──

pub async fn query_events(
    State(state): State<AppState>,
    Query(filter): Query<EventQuery>,
    Query(page): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<EventRow>>, (StatusCode, String)> {
    state
        .svc
        .query_events(filter, page.page.unwrap_or(1), page.page_size.unwrap_or(20))
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

// ── 审计 ──

pub async fn query_audit_logs(
    State(state): State<AppState>,
    Query(filter): Query<AuditLogQuery>,
    Query(page): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<AuditLogRow>>, (StatusCode, String)> {
    state
        .svc
        .query_audit_logs(filter, page.page.unwrap_or(1), page.page_size.unwrap_or(20))
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

// ── 节点 ──

pub async fn list_nodes(
    State(state): State<AppState>,
) -> Result<Json<Vec<NodeApplicationRow>>, (StatusCode, String)> {
    state
        .svc
        .list_nodes()
        .await
        .map(Json)
        .map_err(|e| to_error(e))
}

// ── 数据上报 ──

pub async fn receive_heartbeat(
    State(state): State<AppState>,
    Json(hb): Json<Heartbeat>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .svc
        .receive_heartbeat(hb)
        .await
        .map(|_| StatusCode::OK)
        .map_err(|e| to_error(e))
}

pub async fn receive_metrics(
    State(state): State<AppState>,
    Json(batch): Json<MetricsBatch>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .svc
        .receive_metrics(batch)
        .await
        .map(|_| StatusCode::OK)
        .map_err(|e| to_error(e))
}

pub async fn receive_events(
    State(state): State<AppState>,
    Json(batch): Json<EventsBatch>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .svc
        .receive_events(batch)
        .await
        .map(|_| StatusCode::OK)
        .map_err(|e| to_error(e))
}

// ── 健康检查 ──

pub async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

pub async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

pub async fn readyz(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    // 检查 DB 连接：尝试列出节点
    match state.svc.list_nodes().await {
        Ok(_) => Json(serde_json::json!({"status": "ok"})),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "error": e.to_string()
        })),
    }
}

// ── 错误转换 ──

fn to_error(e: ConrogateError) -> (StatusCode, String) {
    let status = match &e {
        ConrogateError::NotFound(_) => StatusCode::NOT_FOUND,
        ConrogateError::BadRequest(_) => StatusCode::BAD_REQUEST,
        ConrogateError::Conflict(_) | ConrogateError::ConfigConcurrencyConflict => StatusCode::CONFLICT,
        ConrogateError::Unauthorized => StatusCode::UNAUTHORIZED,
        ConrogateError::Forbidden => StatusCode::FORBIDDEN,
        ConrogateError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        ConrogateError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        _ if e.is_internal() => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    // 内部错误不暴露细节
    let message = if e.is_internal() {
        "internal error".to_string()
    } else {
        e.to_string()
    };

    (status, message)
}
