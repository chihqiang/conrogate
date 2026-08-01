//! 数据库迁移。

use conrogate_contract::config::DbConfig;
use conrogate_contract::ConrogateError;

/// 执行数据库迁移
pub async fn run_migrations(_db_config: &DbConfig) -> Result<(), ConrogateError> {
    // TODO: P2 阶段实现 sea-orm-migration
    tracing::info!("migration: not yet implemented (placeholder)");
    Ok(())
}
