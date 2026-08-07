//! 演示数据（mock data）写入。
//!
//! 由 `conrogate-migrate` 在迁移完成后调用；服务二进制
//! （conrogate / conrogate-control / conrogate-gate）不负责数据迁移与演示数据写入。

use sea_orm::DatabaseConnection;

use crate::contract::balancer::BalancerAlgorithm;
use crate::contract::dto::*;
use crate::contract::plugin::{PluginKind, PluginStatus};
use crate::contract::protocol::{PathMatch, ProtocolId, RouteMatchConditions};
use crate::contract::storage::*;
use crate::contract::ConrogateError;
use crate::storage::repository::installed_plugin_repo::InstalledPluginRepoImpl;

/// 写入演示数据：注册官方插件 + 1 个上游 + 1 条演示路由（上游已有数据则跳过）。
pub async fn seed_demo_data(
    main_db: &DatabaseConnection,
    upstream_name: &str,
    upstream_address: &str,
) -> Result<(), ConrogateError> {
    // 注册官方插件（幂等：已存在则跳过）
    let plugin_repo = InstalledPluginRepoImpl::new(main_db.clone());
    seed_official_plugins(&plugin_repo).await?;

    let upstream_repo =
        crate::storage::repository::upstream_repo::UpstreamRepoImpl::new(main_db.clone());
    let route_repo = crate::storage::repository::route_repo::RouteRepoImpl::new(main_db.clone());

    // 检查是否已有数据
    let existing = ReadOnlyUpstreamRepo::list_all(&upstream_repo).await?;
    if !existing.is_empty() {
        tracing::info!("demo data already exists, skipping seed");
        return Ok(());
    }

    // 创建上游（指向演示后端 upstream_address）
    let upstream = upstream_repo
        .create(CreateUpstreamDto {
            name: upstream_name.into(),
            algorithm: BalancerAlgorithm::RoundRobin,
            retry_enabled: Some(false),
            nodes: vec![CreateUpstreamNodeDto {
                address: upstream_address.into(),
                weight: Some(1),
                enabled: Some(true),
            }],
        })
        .await?;

    // 创建演示路由
    let _route = route_repo
        .create(CreateRouteDto {
            name: "demo-route".into(),
            protocol: ProtocolId::Http,
            match_conditions: RouteMatchConditions {
                path: PathMatch::Prefix("/demo/".into()),
                methods: None,
                host: None,
                headers: vec![],
                query_params: vec![],
            },
            priority: Some(10),
            upstream_id: Some(upstream.id),
            host_header: None,
            allow_retry_non_idempotent: Some(false),
            ws_strip_sensitive_headers: Some(false),
            enabled: Some(true),
        })
        .await?;

    tracing::info!(
        upstream_id = upstream.id,
        upstream_name,
        "demo data seeded: {} + demo-route",
        upstream_name
    );
    Ok(())
}

/// 注册编译进二进制的官方插件（幂等）。让控制面「插件管理」页面有数据可查。
async fn seed_official_plugins(
    plugin_repo: &InstalledPluginRepoImpl,
) -> Result<(), ConrogateError> {
    let now = chrono::Utc::now();
    let plugins = [
        (
            "log",
            serde_json::json!({"name": "log", "title": "访问日志", "description": "请求访问日志记录"}),
        ),
        (
            "cors",
            serde_json::json!({"name": "cors", "title": "跨域", "description": "CORS 响应头注入与预检处理"}),
        ),
        (
            "auth",
            serde_json::json!({"name": "auth", "title": "鉴权", "description": "JWT Bearer Token 校验"}),
        ),
    ];
    for (name, manifest) in plugins {
        if plugin_repo.find_by_name(name).await?.is_some() {
            continue;
        }
        plugin_repo
            .insert(&InstalledPluginDto {
                name: name.into(),
                version: "0.0.1".into(),
                api_version: 1,
                kind: PluginKind::Native,
                status: PluginStatus::Active,
                package_hash: None,
                manifest,
                installed_at: now,
                activated_at: Some(now),
            })
            .await?;
    }
    tracing::info!("official plugins seeded: log / cors / auth");
    Ok(())
}
