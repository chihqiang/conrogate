//! axum 路由注册。

use crate::handler::{self, AppState};
use axum::middleware;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use tower_http::trace::TraceLayer;

/// 构建控制面 API 路由
pub fn build_router(state: AppState, auth_token: &str) -> Router {
    let auth_state = crate::auth::AuthState {
        token: auth_token.to_string(),
    };

    // 公开路由（不需要认证）
    let public_routes = Router::new()
        .route("/health", get(handler::health_check))
        .route("/healthz", get(handler::healthz))
        .route("/readyz", get(handler::readyz))
        .route("/openapi.json", get(serve_openapi));

    // 认证路由
    let protected_routes = Router::new()
        // ── 路由管理 ──
        .route("/routes", post(handler::create_route).get(handler::list_routes))
        .route("/routes/:id", get(handler::get_route).put(handler::update_route).delete(handler::delete_route))
        // ── 上游管理 ──
        .route("/upstreams", post(handler::create_upstream).get(handler::list_upstreams))
        .route("/upstreams/:id", get(handler::get_upstream).put(handler::update_upstream).delete(handler::delete_upstream))
        // ── 插件绑定 ──
        .route("/routes/:id/plugins", post(handler::bind_plugin).get(handler::list_plugin_bindings))
        .route("/routes/:id/plugins/:plugin_name", delete(handler::unbind_plugin))
        // ── 配置版本 ──
        .route("/config/publish", post(handler::publish_config))
        .route("/config/rollback", post(handler::rollback_config))
        .route("/config/versions", get(handler::list_config_versions))
        .route("/config/diff", get(handler::diff_config))
        // ── 指标 ──
        .route("/metrics", get(handler::query_metrics))
        .route("/metrics/overview", get(handler::overview_metrics))
        // ── 事件 ──
        .route("/events", get(handler::query_events))
        // ── 审计 ──
        .route("/audit-logs", get(handler::query_audit_logs))
        // ── 节点 ──
        .route("/nodes", get(handler::list_nodes))
        // ── 数据上报（gate → control）──
        .route("/report/heartbeat", post(handler::receive_heartbeat))
        .route("/report/metrics", post(handler::receive_metrics))
        .route("/report/events", post(handler::receive_events))
        .layer(middleware::from_fn_with_state(
            auth_state,
            crate::auth::auth_middleware,
        ));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// API 前缀
pub const API_PREFIX: &str = "/api/v1";

/// 返回 OpenAPI JSON 文档
async fn serve_openapi() -> Json<serde_json::Value> {
    let openapi = crate::openapi::build_openapi();
    Json(serde_json::to_value(&openapi).unwrap_or_default())
}
