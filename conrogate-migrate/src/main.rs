//! Conrogate 数据库迁移工具。
//!
//! 职责：执行数据库迁移（建表/索引），可选写入演示数据（mock data）。
//! 服务二进制（conrogate / conrogate-control / conrogate-gate）不负责迁移与演示数据。
//!
//! 使用方式：
//!   cargo run -p conrogate-migrate              # 仅迁移
//!   cargo run -p conrogate-migrate -- --seed    # 迁移 + 写入演示数据
//!   cargo run -p conrogate-migrate -- --seed --seed-name <name> --seed-address <host:port>

use clap::Parser;

#[derive(Parser)]
#[command(name = "conrogate-migrate")]
#[command(about = "Conrogate 数据库迁移工具（迁移 + 可选演示数据）")]
struct Cli {
    /// 指定 .env 文件路径（默认搜索当前目录 .env）
    #[arg(long, env = "CONROGATE_ENV_FILE")]
    env_file: Option<String>,
    /// 迁移后写入演示数据（默认不写入，仅执行迁移）
    #[arg(long)]
    seed: bool,
    /// 演示上游名称（仅在 --seed 时生效）
    #[arg(long, default_value = "echo-upstream")]
    seed_name: String,
    /// 演示上游地址（仅在 --seed 时生效）
    #[arg(long, default_value = "127.0.0.1:9090")]
    seed_address: String,
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

    // 加载配置
    let config = conrogate_core::contract::config::Config::from_env()
        .map_err(|e| anyhow::anyhow!("config load failed: {e}"))?;

    // 初始化日志（复用统一入口：尊重 RUST_LOG 与 CONROGATE_LOG_* 配置，未设置时默认 INFO）
    conrogate_core::logging::init(&config.log);

    tracing::info!("starting conrogate-migrate");

    // 1. 执行迁移
    conrogate_core::storage::migration::run_migrations(&config.db).await?;
    tracing::info!("migration completed successfully");

    // 2. 写入演示数据（需显式 --seed，默认不写入）
    if cli.seed {
        let main_db = conrogate_core::storage::pool::create_main_pool(&config.db).await?;
        conrogate_core::storage::seed::seed_demo_data(&main_db, &cli.seed_name, &cli.seed_address)
            .await
            .map_err(|e| anyhow::anyhow!("seed demo data failed: {e}"))?;
        tracing::info!("demo data seeded");
    }

    Ok(())
}
