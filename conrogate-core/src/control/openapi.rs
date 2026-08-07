//! OpenAPI 文档生成。
//!
//! 基于 axum 路由与 contract DTO 生成 OpenAPI 3 规范，
//! 暴露于 `GET /openapi.json`。路径表与 `api.rs::build_router` 保持一致。

use crate::contract::balancer::BalancerAlgorithm;
use crate::contract::dto::*;
use crate::contract::plugin::{PluginKind, PluginStatus};
use crate::contract::protocol::{
    HeaderMatch, MatchOp, PathMatch, ProtocolId, QueryMatch, RouteMatchConditions,
};
use utoipa::openapi::content::Content;
use utoipa::openapi::info::InfoBuilder;
use utoipa::openapi::path::{
    HttpMethod, Operation, OperationBuilder, Parameter, ParameterBuilder, ParameterIn,
    PathItemBuilder, Paths, PathsBuilder,
};
use utoipa::openapi::request_body::RequestBodyBuilder;
use utoipa::openapi::response::{Response, ResponseBuilder, Responses, ResponsesBuilder};
use utoipa::openapi::schema::{ObjectBuilder, Ref, Schema, Type};
use utoipa::openapi::tag::{Tag, TagBuilder};
use utoipa::openapi::{Components, ComponentsBuilder, OpenApiBuilder, RefOr, Required};
use utoipa::{PartialSchema, ToSchema};

/// 通用响应体（与 `response.rs::UnifiedResponse` 对齐）
#[derive(ToSchema)]
pub struct ApiResponse {
    /// 业务码，0 表示成功
    pub code: i32,
    /// 提示信息
    pub msg: String,
    /// 业务数据（按端点各有具体 schema，见 components/schemas）
    #[schema(value_type = Object)]
    pub data: Option<serde_json::Value>,
    /// 链路追踪 ID
    pub trace_id: String,
}

fn json_content(schema: Option<RefOr<Schema>>) -> Content {
    Content::new(schema)
}

fn ref_to(name: &str) -> RefOr<Schema> {
    RefOr::Ref(Ref::from_schema_name(name))
}

fn string_schema() -> RefOr<Schema> {
    RefOr::T(Schema::Object(
        ObjectBuilder::new().schema_type(Type::String).build(),
    ))
}

fn integer_schema() -> RefOr<Schema> {
    RefOr::T(Schema::Object(
        ObjectBuilder::new().schema_type(Type::Integer).build(),
    ))
}

/// 路径参数
fn path_param(name: &str, description: &str, schema: RefOr<Schema>) -> Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Path)
        .description(Some(description.to_string()))
        .required(Required::True)
        .schema(Some(schema))
        .build()
}

/// 可选查询参数
fn query_param(name: &str, description: &str, schema: RefOr<Schema>) -> Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Query)
        .description(Some(description.to_string()))
        .required(Required::False)
        .schema(Some(schema))
        .build()
}

/// 必填查询参数
fn query_param_required(name: &str, description: &str, schema: RefOr<Schema>) -> Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Query)
        .description(Some(description.to_string()))
        .required(Required::True)
        .schema(Some(schema))
        .build()
}

fn id_param() -> Parameter {
    path_param("id", "资源 ID", integer_schema())
}

fn page_params() -> Vec<Parameter> {
    vec![
        query_param("page", "页码，默认 1", integer_schema()),
        query_param("page_size", "每页数量，默认 20", integer_schema()),
    ]
}

fn range_min_param() -> Parameter {
    query_param("range_min", "时间范围（分钟），默认 5", integer_schema())
}

/// 成功响应：统一响应体
fn ok_response() -> Response {
    ResponseBuilder::new()
        .description("成功：统一响应体 {code,msg,data,trace_id}")
        .content(
            "application/json",
            json_content(Some(ref_to("ApiResponse"))),
        )
        .build()
}

fn bad_response() -> Response {
    ResponseBuilder::new()
        .description("业务错误：HTTP 200 + code/msg（10001/10004/10005 等）")
        .content(
            "application/json",
            json_content(Some(ref_to("ApiResponse"))),
        )
        .build()
}

fn responses() -> Responses {
    ResponsesBuilder::new()
        .response("200", ok_response())
        .response("400", bad_response())
        .response("401", {
            let mut r = bad_response();
            r.description = "未授权：缺少/无效 Token（code=10002）".to_string();
            r
        })
        .response("403", {
            let mut r = bad_response();
            r.description = "无权限：角色不足（code=10003）".to_string();
            r
        })
        .response("404", {
            let mut r = bad_response();
            r.description = "资源不存在（code=10004）".to_string();
            r
        })
        .build()
}

/// 构建单个操作
#[allow(clippy::too_many_arguments)]
fn op(
    tag: &str,
    summary: &str,
    op_id: &str,
    params: Vec<Parameter>,
    body: Option<(&str, bool)>,
) -> Operation {
    let mut builder = OperationBuilder::new()
        .tag(tag)
        .summary(Some(summary))
        .operation_id(Some(op_id))
        .responses(responses());
    if !params.is_empty() {
        builder = builder.parameters(Some(params));
    }
    if let Some((schema_name, required)) = body {
        let request_body = RequestBodyBuilder::new()
            .description(Some(format!("请求体：{schema_name}")))
            .required(Some(if required {
                Required::True
            } else {
                Required::False
            }))
            .content("application/json", json_content(Some(ref_to(schema_name))))
            .build();
        builder = builder.request_body(Some(request_body));
    }
    builder.build()
}

/// 收集路径表（与 `api.rs::build_router` 的路由注册保持一致）
///
/// 公开路由挂在根路径，受保护路由统一加 `api_prefix` 前缀（与 build_router 一致）。
fn build_paths(api_prefix: &str) -> Paths {
    let mut b = PathsBuilder::new();

    // ── 健康检查（公开）──
    b = b
        .path(
            "/health",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op("health", "健康检查", "health_check", vec![], None),
                )
                .build(),
        )
        .path(
            "/healthz",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op("health", "存活探针", "healthz", vec![], None),
                )
                .build(),
        )
        .path(
            "/readyz",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op("health", "就绪探针", "readyz", vec![], None),
                )
                .build(),
        );

    // ── 路由管理 ──
    b = b
        .path(
            "/routes",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Post,
                    op(
                        "routes",
                        "创建路由",
                        "create_route",
                        vec![],
                        Some(("CreateRouteDto", true)),
                    ),
                )
                .operation(
                    HttpMethod::Get,
                    op("routes", "路由列表", "list_routes", page_params(), None),
                )
                .build(),
        )
        .path(
            "/routes/{id}",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op("routes", "路由详情", "get_route", vec![id_param()], None),
                )
                .operation(
                    HttpMethod::Put,
                    op(
                        "routes",
                        "全量更新路由",
                        "update_route",
                        vec![id_param()],
                        Some(("UpdateRouteDto", true)),
                    ),
                )
                .operation(
                    HttpMethod::Patch,
                    op(
                        "routes",
                        "部分更新路由",
                        "patch_route",
                        vec![id_param()],
                        Some(("UpdateRouteDto", true)),
                    ),
                )
                .operation(
                    HttpMethod::Delete,
                    op("routes", "删除路由", "delete_route", vec![id_param()], None),
                )
                .build(),
        );

    // ── 上游管理 ──
    b = b
        .path(
            "/upstreams",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Post,
                    op(
                        "upstreams",
                        "创建上游",
                        "create_upstream",
                        vec![],
                        Some(("CreateUpstreamDto", true)),
                    ),
                )
                .operation(
                    HttpMethod::Get,
                    op(
                        "upstreams",
                        "上游列表",
                        "list_upstreams",
                        page_params(),
                        None,
                    ),
                )
                .build(),
        )
        .path(
            "/upstreams/{id}",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op(
                        "upstreams",
                        "上游详情",
                        "get_upstream",
                        vec![id_param()],
                        None,
                    ),
                )
                .operation(
                    HttpMethod::Put,
                    op(
                        "upstreams",
                        "全量更新上游",
                        "update_upstream",
                        vec![id_param()],
                        Some(("UpdateUpstreamDto", true)),
                    ),
                )
                .operation(
                    HttpMethod::Patch,
                    op(
                        "upstreams",
                        "部分更新上游",
                        "patch_upstream",
                        vec![id_param()],
                        Some(("UpdateUpstreamDto", true)),
                    ),
                )
                .operation(
                    HttpMethod::Delete,
                    op(
                        "upstreams",
                        "删除上游",
                        "delete_upstream",
                        vec![id_param()],
                        None,
                    ),
                )
                .build(),
        );

    // ── 插件绑定 ──
    b = b
        .path(
            "/routes/{id}/plugins",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Post,
                    op(
                        "plugins",
                        "绑定插件到路由",
                        "bind_plugin",
                        vec![id_param()],
                        Some(("BindPluginDto", true)),
                    ),
                )
                .operation(
                    HttpMethod::Get,
                    op(
                        "plugins",
                        "路由插件绑定列表",
                        "list_plugin_bindings",
                        vec![id_param()],
                        None,
                    ),
                )
                .build(),
        )
        .path(
            "/routes/{id}/plugins/{plugin_name}",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Put,
                    op(
                        "plugins",
                        "更新插件绑定配置",
                        "update_plugin_binding",
                        vec![
                            id_param(),
                            path_param("plugin_name", "插件名称", string_schema()),
                        ],
                        Some(("UpdatePluginBindingDto", true)),
                    ),
                )
                .operation(
                    HttpMethod::Delete,
                    op(
                        "plugins",
                        "解绑插件",
                        "unbind_plugin",
                        vec![
                            id_param(),
                            path_param("plugin_name", "插件名称", string_schema()),
                        ],
                        None,
                    ),
                )
                .build(),
        );

    // ── 配置版本 ──
    b = b
        .path(
            "/configs/publish",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Post,
                    op(
                        "config",
                        "发布配置（生成新版本快照）",
                        "publish_config",
                        vec![
                            query_param("base_version", "基线版本号", integer_schema()),
                            query_param("remark", "发布备注", string_schema()),
                        ],
                        None,
                    ),
                )
                .build(),
        )
        .path(
            "/configs/versions",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op(
                        "config",
                        "配置版本列表",
                        "list_config_versions",
                        page_params(),
                        None,
                    ),
                )
                .build(),
        )
        .path(
            "/configs/versions/{version}/rollback",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Post,
                    op(
                        "config",
                        "回滚到指定版本",
                        "rollback_config",
                        vec![path_param("version", "目标版本号", integer_schema())],
                        None,
                    ),
                )
                .build(),
        )
        .path(
            "/configs/diff",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op(
                        "config",
                        "版本差异对比",
                        "diff_config",
                        vec![
                            query_param_required("from", "起始版本号", integer_schema()),
                            query_param_required("to", "目标版本号", integer_schema()),
                        ],
                        None,
                    ),
                )
                .build(),
        );

    // ── 指标 ──
    b = b
        .path(
            "/metrics",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op(
                        "metrics",
                        "指标查询",
                        "query_metrics",
                        vec![
                            range_min_param(),
                            query_param("route_id", "路由 ID", integer_schema()),
                            query_param("gate_id", "网关节点 ID", string_schema()),
                        ],
                        None,
                    ),
                )
                .build(),
        )
        .path(
            "/metrics/overview",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op(
                        "metrics",
                        "指标总览",
                        "overview_metrics",
                        vec![range_min_param()],
                        None,
                    ),
                )
                .build(),
        );

    // ── Insights 聚合查询 ──
    for (suffix, op_id, summary) in [
        ("overview", "insights_overview", "洞察总览"),
        ("qps", "insights_qps", "QPS 趋势"),
        ("latency", "insights_latency", "延迟趋势"),
        ("status-codes", "insights_status_codes", "状态码分布"),
        ("top-routes", "insights_top_routes", "热门路由排行"),
    ] {
        b = b.path(
            format!("/insights/{suffix}"),
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op("metrics", summary, op_id, vec![range_min_param()], None),
                )
                .build(),
        );
    }

    // ── 事件 / 审计 ──
    b = b
        .path(
            "/insights/events",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op(
                        "events",
                        "事件查询",
                        "query_events",
                        vec![
                            query_param("event_type", "事件类型", string_schema()),
                            query_param("route_id", "路由 ID", integer_schema()),
                            query_param("ts_from", "起始时间（ISO8601）", string_schema()),
                            query_param("ts_to", "结束时间（ISO8601）", string_schema()),
                            query_param("page", "页码，默认 1", integer_schema()),
                            query_param("page_size", "每页数量，默认 20", integer_schema()),
                        ],
                        None,
                    ),
                )
                .build(),
        )
        .path(
            "/audit-logs",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op(
                        "audit",
                        "审计日志查询",
                        "query_audit_logs",
                        vec![
                            query_param("operator", "操作者", string_schema()),
                            query_param("action", "操作动作", string_schema()),
                            query_param("resource", "资源类型", string_schema()),
                            query_param("ts_from", "起始时间（ISO8601）", string_schema()),
                            query_param("ts_to", "结束时间（ISO8601）", string_schema()),
                            query_param("page", "页码，默认 1", integer_schema()),
                            query_param("page_size", "每页数量，默认 20", integer_schema()),
                        ],
                        None,
                    ),
                )
                .build(),
        );

    // ── 节点 ──
    b = b.path(
        "/nodes",
        PathItemBuilder::new()
            .operation(
                HttpMethod::Get,
                op("nodes", "网关节点应用列表", "list_nodes", vec![], None),
            )
            .build(),
    );

    // ── 插件管理（Admin）──
    b = b
        .path(
            "/plugins",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Get,
                    op(
                        "plugins",
                        "已安装插件列表",
                        "list_plugins",
                        vec![query_param("status", "过滤状态", string_schema())],
                        None,
                    ),
                )
                .build(),
        )
        .path(
            "/plugins/{name}/activate",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Post,
                    op(
                        "plugins",
                        "激活插件",
                        "activate_plugin",
                        vec![path_param("name", "插件名称", string_schema())],
                        None,
                    ),
                )
                .build(),
        )
        .path(
            "/plugins/{name}/disable",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Post,
                    op(
                        "plugins",
                        "禁用插件",
                        "disable_plugin",
                        vec![path_param("name", "插件名称", string_schema())],
                        None,
                    ),
                )
                .build(),
        )
        .path(
            "/plugins/{name}",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Delete,
                    op(
                        "plugins",
                        "卸载插件",
                        "delete_plugin",
                        vec![path_param("name", "插件名称", string_schema())],
                        None,
                    ),
                )
                .build(),
        );

    // ── 数据上报（gate → control）──
    b = b
        .path(
            "/reports/heartbeat",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Post,
                    op(
                        "report",
                        "节点心跳上报",
                        "receive_heartbeat",
                        vec![],
                        Some(("Heartbeat", true)),
                    ),
                )
                .build(),
        )
        .path(
            "/reports/metrics",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Post,
                    op(
                        "report",
                        "指标批量上报",
                        "receive_metrics",
                        vec![],
                        Some(("MetricsBatch", true)),
                    ),
                )
                .build(),
        )
        .path(
            "/reports/events",
            PathItemBuilder::new()
                .operation(
                    HttpMethod::Post,
                    op(
                        "report",
                        "事件批量上报",
                        "receive_events",
                        vec![],
                        Some(("EventsBatch", true)),
                    ),
                )
                .build(),
        );

    let api = api_prefix.trim();

    // 重新挂载路径：公开路径保持根路径，受保护路径统一加 api_prefix
    // （与 api.rs::build_router 的 nest 行为保持一致）
    let mut result = PathsBuilder::new();
    for (path, item) in b.build().paths {
        let key = if PUBLIC_PATHS.contains(&path.as_str()) {
            path
        } else {
            format!("{}{}", api, path)
        };
        result = result.path(key, item);
    }
    result.build()
}

/// 公开路径（挂载在根路径，不受 api_prefix 影响）
const PUBLIC_PATHS: [&str; 3] = ["/health", "/healthz", "/readyz"];

/// 注册 DTO schema 到 components
fn build_components() -> Components {
    ComponentsBuilder::new()
        .schema("ApiResponse", ApiResponse::schema())
        // 路由
        .schema("RouteDto", RouteDto::schema())
        .schema("CreateRouteDto", CreateRouteDto::schema())
        .schema("UpdateRouteDto", UpdateRouteDto::schema())
        // 上游
        .schema("UpstreamDto", UpstreamDto::schema())
        .schema("UpstreamNodeDto", UpstreamNodeDto::schema())
        .schema("CreateUpstreamDto", CreateUpstreamDto::schema())
        .schema("CreateUpstreamNodeDto", CreateUpstreamNodeDto::schema())
        .schema("UpdateUpstreamDto", UpdateUpstreamDto::schema())
        // 插件绑定
        .schema("PluginBindingDto", PluginBindingDto::schema())
        .schema("BindPluginDto", BindPluginDto::schema())
        .schema("UpdatePluginBindingDto", UpdatePluginBindingDto::schema())
        // 配置版本
        .schema("ConfigVersionDto", ConfigVersionDto::schema())
        .schema("PublishType", PublishType::schema())
        .schema("ConfigSnapshot", ConfigSnapshot::schema())
        .schema("ConfigDiff", ConfigDiff::schema())
        // 指标与事件
        .schema("MetricRow", MetricRow::schema())
        .schema("EventRow", EventRow::schema())
        .schema("OverviewMetric", OverviewMetric::schema())
        .schema("MetricQuery", MetricQuery::schema())
        .schema("EventQuery", EventQuery::schema())
        .schema("AuditLogQuery", AuditLogQuery::schema())
        // 审计 / 节点 / 插件
        .schema("AuditLogRow", AuditLogRow::schema())
        .schema("NodeApplicationRow", NodeApplicationRow::schema())
        .schema("InstalledPluginDto", InstalledPluginDto::schema())
        // 数据上报
        .schema("MetricsBatch", MetricsBatch::schema())
        .schema("EventsBatch", EventsBatch::schema())
        .schema("Heartbeat", Heartbeat::schema())
        // 契约基础类型
        .schema("ProtocolId", ProtocolId::schema())
        .schema("PathMatch", PathMatch::schema())
        .schema("MatchOp", MatchOp::schema())
        .schema("HeaderMatch", HeaderMatch::schema())
        .schema("QueryMatch", QueryMatch::schema())
        .schema("RouteMatchConditions", RouteMatchConditions::schema())
        .schema("BalancerAlgorithm", BalancerAlgorithm::schema())
        .schema("PluginKind", PluginKind::schema())
        .schema("PluginStatus", PluginStatus::schema())
        // 分页结果（泛型实例）
        .schema(
            "PaginatedResult",
            PaginatedResult::<serde_json::Value>::schema(),
        )
        .build()
}

fn build_tags() -> Vec<Tag> {
    let definitions = [
        ("health", "健康检查端点"),
        ("routes", "路由管理 CRUD"),
        ("upstreams", "上游管理 CRUD"),
        ("plugins", "插件绑定与插件管理"),
        ("config", "配置版本发布/回滚/差异"),
        ("metrics", "指标与洞察查询"),
        ("events", "事件查询"),
        ("audit", "审计日志查询"),
        ("nodes", "节点应用列表"),
        ("report", "gate → control 数据上报"),
    ];
    definitions
        .into_iter()
        .map(|(name, desc)| {
            TagBuilder::new()
                .name(name)
                .description(Some(desc.to_string()))
                .build()
        })
        .collect()
}

/// 构建 OpenAPI 文档
pub fn build_openapi(api_prefix: &str) -> utoipa::openapi::OpenApi {
    let info = InfoBuilder::new()
        .title("Conrogate Control Plane API")
        .version("0.1.0")
        .description(Some(
            "Conrogate 网关控制面 REST API — 路由/上游/插件/配置版本/指标/事件/审计/数据上报"
                .to_string(),
        ))
        .build();

    OpenApiBuilder::new()
        .info(info)
        .paths(build_paths(api_prefix))
        .components(Some(build_components()))
        .tags(Some(build_tags()))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_has_paths_and_schemas() {
        let doc = build_openapi("/api/v1");
        assert!(!doc.paths.paths.is_empty(), "openapi must contain paths");
        let components = doc
            .components
            .as_ref()
            .expect("openapi must have components");
        assert!(components.schemas.contains_key("RouteDto"));
        assert!(components.schemas.contains_key("MetricsBatch"));
        assert!(components.schemas.contains_key("Heartbeat"));
        // 公开路径保持在根路径
        assert!(doc.paths.paths.contains_key("/health"));
        // 受保护端点必须带前缀
        for p in [
            "/api/v1/routes",
            "/api/v1/routes/{id}",
            "/api/v1/reports/metrics",
            "/api/v1/insights/events",
        ] {
            assert!(doc.paths.paths.contains_key(p), "missing path {p}");
        }
    }
}
