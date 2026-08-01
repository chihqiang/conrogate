//! Conrogate 数据面专用二进制（分离模式）。
//!
//! 仅运行数据面（路由→插件→负载均衡→转发），不监听控制面端口。
//! 配置来源：Redis 缓存（优先）/ HTTP 从 control 拉取 / 直连 DB 只读。

use clap::Parser;
use std::sync::Arc;

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

    // ── 1. 只读 DB 连接池 ──
    let read_db = match conrogate_storage::pool::create_read_pool(&config.db).await {
        Ok(db) => Arc::new(db),
        Err(e) => {
            tracing::warn!(error = %e, "read db pool failed, starting with empty config");
            return run_without_db(config).await;
        }
    };

    // ── 2. 加载初始配置 ──
    let route_repo = conrogate_storage::repository::route_repo::RouteRepoImpl::new((*read_db).clone());
    let upstream_repo = conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new((*read_db).clone());

    let routes = conrogate_contract::storage::ReadOnlyRouteRepo::list_enabled(&route_repo).await
        .unwrap_or_default();
    let upstreams = conrogate_contract::storage::ReadOnlyUpstreamRepo::list_all(&upstream_repo).await
        .unwrap_or_default();

    // ── 3. 创建 GatewayServer（注册插件 + 装配 ServiceContext）──
    let server = conrogate_gateway::server::GatewayServer::from_config(config.clone()).await;

    // ── 4. 加载路由 + 上游 ──
    server.reload_routes(routes);
    server.reload_upstreams(upstreams);

    // ── 5. 启动数据面监听 ──
    server.run().await.map_err(|e| anyhow::anyhow!("gateway server error: {e}"))?;

    Ok(())
}

/// 无 DB 模式启动（仅 HTTP 拉取配置）
async fn run_without_db(config: conrogate_contract::config::Config) -> anyhow::Result<()> {
    tracing::info!("starting gate without db (http config mode)");

    let server = conrogate_gateway::server::GatewayServer::from_config(config).await;
    server.run().await.map_err(|e| anyhow::anyhow!("gateway server error: {e}"))?;

    Ok(())
}
