//! Conrogate 合并二进制（数据面 + 控制面同进程，单机模式）。
//!
//! 双端口监听：8080 数据面 + 9000 控制面。

mod bootstrap;

use clap::Parser;

#[derive(Parser)]
#[command(name = "conrogate")]
#[command(about = "Conrogate 网关 — 合并模式（数据面 + 控制面同进程）")]
struct Cli {
    /// 指定 .env 文件路径
    #[arg(long)]
    env_file: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 加载 .env（可选）
    if let Some(path) = cli.env_file {
        let _ = dotenvy::from_path(&path);
    } else {
        let _ = dotenvy::dotenv();
    }

    // 加载配置
    let config = conrogate_contract::config::Config::from_env()
        .map_err(|e| anyhow::anyhow!("config load failed: {e}"))?;
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("config validation failed: {e}"))?;

    // 初始化日志（JSON 格式 + 文件输出）
    conrogate_gateway::logging::init(&config.log);

    tracing::info!(
        instance_id = ?config.common.instance_id,
        gate_port = config.gate.listen.port,
        control_port = config.control.listen.port,
        worker_threads = config.gate.worker_threads,
        "starting conrogate (merged mode)"
    );

    // 按配置 worker_threads 构建 tokio 运行时（0 = 自动取 CPU 核数）
    let runtime = build_runtime(config.gate.worker_threads)?;
    runtime.block_on(run(config))
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

async fn run(config: conrogate_contract::config::Config) -> anyhow::Result<()> {
    // Bootstrap 装配
    let shutdown_tx = bootstrap::run(config).await?;

    // 等待停机信号
    tokio::signal::ctrl_c().await?;
    tracing::info!("received SIGINT, initiating graceful shutdown");

    // 发送停机信号（broadcast 通知 gate + 后台任务）
    let _ = shutdown_tx.send(());

    // 等待优雅停机完成（最多 35s）
    tokio::time::sleep(std::time::Duration::from_secs(35)).await;
    tracing::info!("shutdown complete");

    Ok(())
}
