//! axum 路由注册。

use super::handler::{self, AppState};
use super::trace;
use axum::body::Body;
use axum::extract::State;
use axum::middleware;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use tower_http::trace::TraceLayer;
use tracing::Span;

/// 构建控制面 API 路由
///
/// 公开路由（health/healthz/readyz/openapi.json）挂在根路径；
/// 受保护路由挂载在 `api_prefix` 下（默认 `/api/v1`）。
///
/// 中间件顺序（外→内）：trace 中间件 → TraceLayer → 认证中间件。
/// trace 中间件在最外层，负责提取/生成 trace_id 并写入请求头，使内层 TraceLayer
/// 的日志 span 与响应信封、`x-trace-id` 响应头、审计共用同一 trace_id。
pub fn build_router(state: AppState, auth_token: &str, api_prefix: &str) -> Router {
    let auth_state = super::auth::AuthState::from_configured(auth_token);

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
        // ── 全局 IP 黑名单 ──
        .route(
            "/security/ip_blacklist",
            post(handler::create_ip_blacklist).get(handler::list_ip_blacklist),
        )
        .route(
            "/security/ip_blacklist/:id",
            axum::routing::delete(handler::delete_ip_blacklist),
        )
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
            super::auth::auth_middleware,
        ));

    // 将受保护路由挂载到 api_prefix 下（空前缀则保留根路径）
    let protected_routes = if api_prefix.trim().is_empty() {
        protected_routes
    } else {
        Router::new().nest(api_prefix, protected_routes)
    };

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        // 日志 span：记录 trace_id（trace 中间件已写入请求头）
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<Body>| {
                    let method = request.method();
                    let uri = request.uri();
                    tracing::info_span!(
                        "http_request",
                        method = %method,
                        uri = %uri,
                        trace_id = tracing::field::Empty,
                    )
                })
                .on_request(|request: &axum::http::Request<Body>, span: &Span| {
                    let trace_id = request
                        .headers()
                        .get(crate::contract::constant::TRACE_ID_HEADER)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or_default()
                        .to_string();
                    span.record("trace_id", trace_id);
                }),
        )
        // trace 中间件在最外层：先于 TraceLayer 提取/生成 trace_id 并写入请求头
        .layer(middleware::from_fn_with_state(
            state.clone(),
            trace::trace_middleware,
        ))
        .with_state(state)
}

/// 返回 OpenAPI JSON 文档
async fn serve_openapi(State(state): State<AppState>) -> Json<serde_json::Value> {
    let openapi = super::openapi::build_openapi(&state.api_prefix);
    Json(serde_json::to_value(&openapi).unwrap_or_default())
}
