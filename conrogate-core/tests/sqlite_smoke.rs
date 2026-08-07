//! SQLite 方言冒烟测试：真实跑迁移 + 仓储往返 + 原生 SQL 路径。
//!
//! 用临时文件库（并发安全，多连接共享同一库），验证 SeaORM SQLite 后端与
//! 声明式迁移的兼容性（BIGINT 自增主键、partial unique index、JSON 列、
//! 时间戳默认值、外键等）。

use conrogate_core::contract::{
    balancer::BalancerAlgorithm,
    dto::*,
    protocol::{PathMatch, ProtocolId, RouteMatchConditions},
    storage::*,
    ConrogateError,
};
use conrogate_core::storage::{
    config_cache::DbConfigCache,
    migration::ConrogateMigrator,
    repository::{
        config_version_repo::ConfigVersionRepoImpl, plugin_binding_repo::PluginBindingRepoImpl,
        route_repo::RouteRepoImpl, upstream_repo::UpstreamRepoImpl,
    },
};
use sea_orm::Database;
use sea_orm_migration::MigratorTrait;
use std::sync::Arc;

/// 测试用临时库路径；结束后清理。并行测试用 pid 区分避免冲突。
fn db_url() -> (String, std::path::PathBuf) {
    let path =
        std::env::temp_dir().join(format!("conrogate_sqlite_smoke_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    (format!("sqlite://{}?mode=rwc", path.display()), path)
}

struct TempDbGuard(std::path::PathBuf);

impl Drop for TempDbGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn route_conditions() -> RouteMatchConditions {
    RouteMatchConditions {
        path: PathMatch::Prefix("/demo/".into()),
        methods: None,
        host: None,
        headers: vec![],
        query_params: vec![],
    }
}

fn create_route_dto(name: &str, upstream_id: u64) -> CreateRouteDto {
    CreateRouteDto {
        name: name.into(),
        protocol: ProtocolId::Http,
        match_conditions: route_conditions(),
        priority: Some(10),
        upstream_id: Some(upstream_id),
        host_header: None,
        allow_retry_non_idempotent: Some(false),
        ws_strip_sensitive_headers: Some(false),
        enabled: Some(true),
    }
}

#[tokio::test]
async fn sqlite_migrate_and_repo_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let (url, path) = db_url();
    let _guard = TempDbGuard(path);

    let db = Database::connect(&url).await?;
    ConrogateMigrator::up(&db, None).await?;

    let route_repo = RouteRepoImpl::new(db.clone());
    let upstream_repo = UpstreamRepoImpl::new(db.clone());
    let binding_repo = PluginBindingRepoImpl::new(db.clone());
    let config_repo = ConfigVersionRepoImpl::new(db.clone());

    // ── 上游 + 节点 ──
    let upstream = upstream_repo
        .create(CreateUpstreamDto {
            name: "echo".into(),
            algorithm: BalancerAlgorithm::RoundRobin,
            retry_enabled: Some(true),
            nodes: vec![CreateUpstreamNodeDto {
                address: "127.0.0.1:9090".into(),
                weight: Some(1),
                enabled: Some(true),
            }],
        })
        .await?;
    assert_eq!(upstream.nodes.len(), 1);

    // ── 路由 ──
    let route = route_repo
        .create(create_route_dto("demo", upstream.id))
        .await?;
    let route_id = route.id;

    // 重复路由名 → Conflict（应用层预检查 + partial index 兜底）
    let dup = route_repo
        .create(create_route_dto("demo", upstream.id))
        .await;
    assert!(matches!(dup, Err(ConrogateError::Conflict(_))));

    // ── 插件绑定 ──
    let binding = binding_repo
        .bind(
            route_id,
            BindPluginDto {
                plugin_name: "cors".into(),
                config: serde_json::json!({}),
                order: Some(1),
                blocking: Some(true),
                enabled: Some(true),
            },
        )
        .await?;
    assert_eq!(binding.plugin_name, "cors");

    // 重复绑定 → Conflict
    let dup_bind = binding_repo
        .bind(
            route_id,
            BindPluginDto {
                plugin_name: "cors".into(),
                config: serde_json::json!({}),
                order: Some(1),
                blocking: Some(true),
                enabled: Some(true),
            },
        )
        .await;
    assert!(matches!(dup_bind, Err(ConrogateError::Conflict(_))));

    // ── 读回校验 ──
    let routes = ReadOnlyRouteRepo::list_enabled(&route_repo).await?;
    assert_eq!(routes.len(), 1);
    assert!(matches!(
        routes[0].match_conditions.path,
        PathMatch::Prefix(ref p) if p == "/demo/"
    ));
    let upstreams = ReadOnlyUpstreamRepo::list_all(&upstream_repo).await?;
    assert_eq!(upstreams.len(), 1);
    let bindings = ReadOnlyPluginBindingRepo::list_by_route(&binding_repo, route_id).await?;
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].config, serde_json::json!({}));

    // ── 配置版本发布（JSON 快照 + 版本唯一索引）──
    let upstream_id = upstream.id;
    let snapshot = ConfigSnapshot {
        routes: vec![route],
        upstreams: vec![upstream],
        plugin_bindings: vec![binding],
    };
    let v1 = config_repo.publish(0, &snapshot, None, None).await?;
    assert_eq!(v1.version, 1);
    let v2 = config_repo.publish(1, &snapshot, None, None).await?;
    assert_eq!(v2.version, 2);
    assert_eq!(config_repo.latest_version().await?.unwrap().version, 2);
    let snap = config_repo.get_snapshot_by_version(1).await?.unwrap();
    assert_eq!(snap.routes.len(), 1);

    // ── DbConfigCache::get_version 原生 SQL（SELECT MAX(version)）──
    let cache = DbConfigCache::new(Arc::new(db.clone()));
    assert_eq!(cache.get_version().await?, Some(2));

    // ── 软删除后同名重建（partial index 语义）──
    route_repo.soft_delete(route_id).await?;
    let route2 = route_repo
        .create(create_route_dto("demo", upstream_id))
        .await?;
    assert_ne!(route2.id, route_id);
    assert_eq!(ReadOnlyRouteRepo::list_enabled(&route_repo).await?.len(), 1);

    Ok(())
}
