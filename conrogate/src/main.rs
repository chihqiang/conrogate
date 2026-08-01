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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 加载 .env（可选）
    if let Some(path) = cli.env_file {
        let _ = dotenvy::from_path(&path);
    } else {
        let _ = dotenvy::dotenv();
    }

    // 初始化日志
    tracing_subscriber::fmt::init();

    // 加载配置
    let config = conrogate_contract::config::Config::from_env()
        .map_err(|e| anyhow::anyhow!("config load failed: {e}"))?;
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("config validation failed: {e}"))?;

    tracing::info!(
        instance_id = ?config.common.instance_id,
        gate_port = config.gate.listen.port,
        control_port = config.control.listen.port,
        "starting conrogate (merged mode)"
    );

    // Bootstrap 装配
    let shutdown_tx = bootstrap::run(config).await?;

    // 等待停机信号
    tokio::signal::ctrl_c().await?;
    tracing::info!("received SIGINT, initiating graceful shutdown");

    // 发送停机信号
    let _ = shutdown_tx.send(());

    // TODO: 等待优雅停机完成（带超时）
    tracing::info!("shutdown complete");

    Ok(())
}
