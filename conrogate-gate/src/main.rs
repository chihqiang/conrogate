//! Conrogate 数据面专用二进制（分离模式）。
//!
//! 仅运行数据面（路由→插件→负载均衡→转发），不监听控制面端口。
//! 配置来源：Redis 缓存（优先）/ HTTP 从 control 拉取 / 直连 DB 只读。

use clap::Parser;

#[derive(Parser)]
#[command(name = "conrogate-gate")]
#[command(about = "Conrogate 数据面专用二进制")]
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
        host = %config.gate.listen.host,
        port = config.gate.listen.port,
        "starting conrogate-gate (data plane only)"
    );

    // TODO: Bootstrap 数据面组件
    // 1. 初始化 BalancerRegistry + HealthChecker + ServiceDiscovery
    // 2. 初始化 TrafficControl + PluginRegistry + PluginPipeline
    // 3. 组装 RouteLookup + TelemetryReport
    // 4. 组装 ServiceContext
    // 5. 注册 ProtocolHandler
    // 6. 启动数据面监听
    // 7. 启动后台任务（配置热加载、指标聚合、心跳）

    tracing::info!("conrogate-gate ready");

    // 等待停机信号
    tokio::signal::ctrl_c().await?;
    tracing::info!("received SIGINT, shutting down");

    Ok(())
}
