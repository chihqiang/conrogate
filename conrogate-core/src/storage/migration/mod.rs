//! 数据库迁移。

use crate::contract::config::DbConfig;
use crate::contract::ConrogateError;
use sea_orm::{ConnectOptions, Database};
use sea_orm_migration::MigratorTrait;

mod m20260101_000001_upstreams;
mod m20260101_000002_upstream_nodes;
mod m20260101_000003_routes;
mod m20260101_000004_route_plugin_bindings;
mod m20260101_000005_config_versions;
mod m20260101_000006_metric_aggregates;
mod m20260101_000007_gateway_events;
mod m20260101_000008_audit_logs;
mod m20260101_000009_node_applications;
mod m20260101_000010_installed_plugins;
mod m20260101_000011_ip_blacklist;

/// 迁移器
pub struct ConrogateMigrator;

impl MigratorTrait for ConrogateMigrator {
    fn migrations() -> Vec<Box<dyn sea_orm_migration::MigrationTrait>> {
        vec![
            Box::new(m20260101_000001_upstreams::Migration),
            Box::new(m20260101_000002_upstream_nodes::Migration),
            Box::new(m20260101_000003_routes::Migration),
            Box::new(m20260101_000004_route_plugin_bindings::Migration),
            Box::new(m20260101_000005_config_versions::Migration),
            Box::new(m20260101_000006_metric_aggregates::Migration),
            Box::new(m20260101_000007_gateway_events::Migration),
            Box::new(m20260101_000008_audit_logs::Migration),
            Box::new(m20260101_000009_node_applications::Migration),
            Box::new(m20260101_000010_installed_plugins::Migration),
            Box::new(m20260101_000011_ip_blacklist::Migration),
        ]
    }
}

/// 执行数据库迁移
pub async fn run_migrations(db_config: &DbConfig) -> Result<(), ConrogateError> {
    let url = db_config.database_url();
    let mut opt = ConnectOptions::new(url.clone());
    // SQLite 文件不存在时自动创建（与主应用连接池行为一致，见 storage/pool.rs）
    if url.starts_with("sqlite:") {
        opt.map_sqlx_sqlite_opts(|o| o.create_if_missing(true));
    }
    opt.connect_timeout(db_config.connect_timeout);

    let db = Database::connect(opt)
        .await
        .map_err(|e| ConrogateError::Migration(format!("db connect failed: {e}")))?;

    ConrogateMigrator::up(&db, None)
        .await
        .map_err(|e| ConrogateError::Migration(format!("migration failed: {e}")))?;

    tracing::info!("database migration completed");
    Ok(())
}
