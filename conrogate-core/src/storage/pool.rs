//! 数据库连接池管理。

use crate::contract::config::DbConfig;
use crate::contract::ConrogateError;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};

/// 数据库连接类型别名
pub type DbConn = DatabaseConnection;

/// 主库连接池（读写）
pub async fn create_main_pool(db_config: &DbConfig) -> Result<DatabaseConnection, ConrogateError> {
    let url = db_config.database_url();
    let mut opt = ConnectOptions::new(url.clone());
    if url.starts_with("sqlite:") {
        opt.map_sqlx_sqlite_opts(|o| o.create_if_missing(true));
    }
    opt.connect_timeout(db_config.connect_timeout)
        .max_connections(db_config.max_connections)
        .min_connections(1)
        .sqlx_logging(false);
    let db = Database::connect(opt)
        .await
        .map_err(|e| ConrogateError::Init(format!("main db pool failed: {e}")))?;
    tracing::info!("main db pool connected");
    Ok(db)
}

/// 只读库连接池（gate 组件使用）
pub async fn create_read_pool(db_config: &DbConfig) -> Result<DatabaseConnection, ConrogateError> {
    let url = db_config.read_database_url();

    let mut opt = ConnectOptions::new(url.clone());
    if url.starts_with("sqlite:") {
        opt.map_sqlx_sqlite_opts(|o| o.create_if_missing(true));
    }
    opt.connect_timeout(db_config.connect_timeout)
        .max_connections(db_config.max_connections)
        .min_connections(1)
        .sqlx_logging(false);

    let db = Database::connect(opt)
        .await
        .map_err(|e| ConrogateError::Init(format!("read db pool failed: {e}")))?;
    tracing::info!("read db pool connected");
    Ok(db)
}
