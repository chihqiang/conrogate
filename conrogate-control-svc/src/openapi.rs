//! OpenAPI 文档生成。

use utoipa::{OpenApi, ToSchema};

/// API 通用响应
#[derive(ToSchema)]
pub struct ApiResponse {
    pub code: i32,
    pub msg: String,
}

/// 构建 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Conrogate Control Plane API",
        version = "0.1.0",
        description = "Conrogate 网关控制面 REST API — 路由/上游/插件/配置版本/指标/事件/审计/数据上报",
    ),
    components(schemas(ApiResponse)),
    tags(
        (name = "health", description = "健康检查端点"),
        (name = "routes", description = "路由管理 CRUD"),
        (name = "upstreams", description = "上游管理 CRUD"),
        (name = "plugins", description = "插件绑定管理"),
        (name = "config", description = "配置版本发布/回滚/差异"),
        (name = "metrics", description = "指标查询"),
        (name = "events", description = "事件查询"),
        (name = "audit", description = "审计日志查询"),
        (name = "nodes", description = "节点应用列表"),
        (name = "report", description = "gate → control 数据上报"),
    )
)]
pub struct ApiDoc;

/// 获取 OpenAPI JSON
pub fn build_openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}
