//! Conrogate 控制面专用二进制（分离模式）。
//!
//! 仅运行控制面（管理 API + 配置落库 + 指标入库 + 审计）。

use clap::Parser;

#[derive(Parser)]
#[command(name = "conrogate-control")]
#[command(about = "Conrogate 控制面专用二进制")]
struct Cli {
    #[arg(long)]
    env_file: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Some(path) = cli.env_file {
        let _ = dotenvy::from_path(&path);
    } else {
        let _ = dotenvy::dotenv();
    }

    tracing_subscriber::fmt::init();

    let config = conrogate_contract::config::Config::from_env()
        .map_err(|e| anyhow::anyhow!("config load failed: {e}"))?;
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("config validation failed: {e}"))?;

    tracing::info!(
        host = %config.control.listen.host,
        port = config.control.listen.port,
        "starting conrogate-control (control plane only)"
    );

    // TODO: Bootstrap 控制面组件
    // 1. 初始化 DB 连接池（主库读写）
    // 2. 初始化仓储层
    // 3. 初始化 ConfigCache（Redis 写入端）
    // 4. 组装 axum 路由 + 中间件
    // 5. 启动控制面监听
    // 6. 启动后台任务（过期数据清理、Redis 缓存刷新）

    tracing::info!("conrogate-control ready");

    tokio::signal::ctrl_c().await?;
    tracing::info!("received SIGINT, shutting down");

    Ok(())
}
