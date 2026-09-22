//! `conrogate migrate` — 数据库迁移工具。
//!
//! 职责：执行数据库迁移（建表/索引），可选写入演示数据（mock data）。
//!
//! 使用方式：
//!   conrogate migrate              # 仅迁移
//!   conrogate migrate --seed       # 迁移 + 写入演示数据
//!   conrogate migrate --seed --seed-name <name> --seed-address <host:port>

pub fn run(
    config: conrogate_core::config::Config,
    seed: bool,
    seed_name: String,
    seed_address: String,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("tokio runtime build failed: {e}"))?;

    runtime.block_on(async_run(config, seed, seed_name, seed_address))
}

async fn async_run(
    config: conrogate_core::config::Config,
    seed: bool,
    seed_name: String,
    seed_address: String,
) -> anyhow::Result<()> {
    tracing::info!("starting conrogate migrate");

    // 1. 执行迁移
    conrogate_storage::migration::run_migrations(&config.db).await?;
    tracing::info!("migration completed successfully");

    // 2. 写入演示数据（需显式 --seed，默认不写入）
    if seed {
        let main_db = conrogate_storage::pool::create_main_pool(&config.db).await?;
        conrogate_storage::seed::seed_demo_data(&main_db, &seed_name, &seed_address)
            .await
            .map_err(|e| anyhow::anyhow!("seed demo data failed: {e}"))?;
        tracing::info!("demo data seeded");
    }

    Ok(())
}
