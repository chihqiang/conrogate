//! Conrogate 数据库迁移工具。
//!
//! 使用方式：cargo run -p conrogate-migrate

use clap::Parser;

#[derive(Parser)]
#[command(name = "conrogate-migrate")]
#[command(about = "Conrogate 数据库迁移工具")]
struct Cli {
    /// 指定 .env 文件路径（默认搜索当前目录 .env）
    #[arg(long, env = "CONROGATE_ENV_FILE")]
    env_file: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 加载 .env 文件（可选）
    if let Some(path) = cli.env_file {
        let _ = dotenvy::from_path(&path);
    } else {
        let _ = dotenvy::dotenv();
    }

    // 初始化日志
    tracing_subscriber::fmt::init();

    tracing::info!("starting conrogate-migrate");

    // 加载配置
    let config = conrogate_contract::config::Config::from_env()
        .map_err(|e| anyhow::anyhow!("config load failed: {e}"))?;

    // 执行迁移
    conrogate_storage::migration::run_migrations(&config.db).await?;

    tracing::info!("migration completed successfully");
    Ok(())
}
