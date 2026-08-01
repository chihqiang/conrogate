//! 数据库迁移。

use conrogate_contract::config::DbConfig;
use conrogate_contract::ConrogateError;
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
    let mut opt = ConnectOptions::new(url);
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
