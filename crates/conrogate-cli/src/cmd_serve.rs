//! `conrogate serve` — 合并模式（数据面 + 控制面同进程）。
//!
//! 双端口监听：8080 数据面 + 9000 控制面。

use crate::bootstrap;

pub fn run(config: conrogate_core::config::Config) -> anyhow::Result<()> {
    tracing::info!(
        instance_id = ?config.common.instance_id,
        gate_port = config.gate.listen.port,
        control_port = config.control.listen.port,
        worker_threads = config.gate.worker_threads,
        "starting conrogate serve (merged mode)"
    );

    let runtime = build_runtime(config.gate.worker_threads)?;
    runtime.block_on(async_run(config))
}

fn build_runtime(worker_threads: usize) -> anyhow::Result<tokio::runtime::Runtime> {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    if worker_threads > 0 {
        builder.worker_threads(worker_threads);
    }
    builder
        .build()
        .map_err(|e| anyhow::anyhow!("tokio runtime build failed: {e}"))
}

async fn async_run(config: conrogate_core::config::Config) -> anyhow::Result<()> {
    let shutdown_tx = bootstrap::run(config).await?;

    tokio::signal::ctrl_c().await?;
    tracing::info!("received SIGINT, initiating graceful shutdown");

    let _ = shutdown_tx.send(());
    tokio::time::sleep(std::time::Duration::from_secs(35)).await;
    tracing::info!("shutdown complete");

    Ok(())
}
