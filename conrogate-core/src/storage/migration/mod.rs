//! 数据库迁移。

use crate::contract::config::DbConfig;
use crate::contract::ConrogateError;
use sea_orm::{ConnectOptions, Database};
use sea_orm_migration::MigratorTrait;

mod m20260101_000001_init;

/// 迁移器
pub struct ConrogateMigrator;

impl MigratorTrait for ConrogateMigrator {
    fn migrations() -> Vec<Box<dyn sea_orm_migration::MigrationTrait>> {
        vec![Box::new(m20260101_000001_init::Migration)]
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
