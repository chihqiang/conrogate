//! axum 路由注册。

use crate::handler::{self, AppState};
use axum::middleware;
use axum::routing::{get, post, put};
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
        .route(
            "/routes",
            post(handler::create_route).get(handler::list_routes),
        )
        .route(
            "/routes/:id",
            get(handler::get_route)
                .put(handler::update_route)
                .patch(handler::patch_route)
                .delete(handler::delete_route),
        )
        // ── 上游管理 ──
        .route(
            "/upstreams",
            post(handler::create_upstream).get(handler::list_upstreams),
        )
        .route(
            "/upstreams/:id",
            get(handler::get_upstream)
                .put(handler::update_upstream)
                .patch(handler::patch_upstream)
                .delete(handler::delete_upstream),
        )
        // ── 插件绑定 ──
        .route(
            "/routes/:id/plugins",
            post(handler::bind_plugin).get(handler::list_plugin_bindings),
        )
        .route(
            "/routes/:id/plugins/:plugin_name",
            put(handler::update_plugin_binding).delete(handler::unbind_plugin),
        )
        // ── 配置版本 ──
        .route("/configs/publish", post(handler::publish_config))
        .route("/configs/versions", get(handler::list_config_versions))
        .route(
            "/configs/versions/:version/rollback",
            post(handler::rollback_config),
        )
        .route("/configs/diff", get(handler::diff_config))
        // ── 指标 ──
        .route("/metrics", get(handler::query_metrics))
        .route("/metrics/overview", get(handler::overview_metrics))
        // ── Insights 聚合查询 ──
        .route("/insights/overview", get(handler::insights_overview))
        .route("/insights/qps", get(handler::insights_qps))
        .route("/insights/latency", get(handler::insights_latency))
        .route(
            "/insights/status-codes",
            get(handler::insights_status_codes),
        )
        .route("/insights/top-routes", get(handler::insights_top_routes))
        // ── 事件 ──
        .route("/insights/events", get(handler::query_events))
        // ── 审计 ──
        .route("/audit-logs", get(handler::query_audit_logs))
        // ── 节点 ──
        .route("/nodes", get(handler::list_nodes))
        // ── 插件管理（Admin 专属写操作）──
        .route("/plugins", get(handler::list_plugins))
        .route("/plugins/:name/activate", post(handler::activate_plugin))
        .route("/plugins/:name/disable", post(handler::disable_plugin))
        .route(
            "/plugins/:name",
            axum::routing::delete(handler::delete_plugin),
        )
        // ── 数据上报（gate → control）──
        .route("/reports/heartbeat", post(handler::receive_heartbeat))
        .route("/reports/metrics", post(handler::receive_metrics))
        .route("/reports/events", post(handler::receive_events))
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
