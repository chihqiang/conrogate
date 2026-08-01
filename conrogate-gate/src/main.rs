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

    // 先加载配置
    let config = conrogate_contract::config::Config::from_env()
        .map_err(|e| anyhow::anyhow!("config load failed: {e}"))?;
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("config validation failed: {e}"))?;

    // 初始化日志（JSON 格式 + 文件输出）
    conrogate_gateway::logging::init(&config.log);

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

    // ── 4a. 节点心跳上报后台任务（分离模式）──
    let control_url = &config.gate.refresh.control_api_url;
    let control_token = config.gate.refresh.control_api_token.clone();
    let gate_id = config.common.instance_id.clone();
    if !control_url.is_empty() {
        let url = control_url.clone();
        tokio::spawn(async move {
            let client = hyper_util::client::legacy::Client::builder(
                hyper_util::rt::TokioExecutor::new(),
            )
            .build(hyper_util::client::legacy::connect::HttpConnector::new());
            let heartbeat_interval = std::time::Duration::from_secs(30);
            loop {
                tokio::time::sleep(heartbeat_interval).await;
                let hb = conrogate_contract::dto::Heartbeat {
                    gate_id: gate_id.clone(),
                    version: 0,
                    timestamp: chrono::Utc::now(),
                };
                let body = serde_json::to_vec(&hb).unwrap_or_default();
                let req = http::Request::builder()
                    .method("POST")
                    .uri(format!("{}/api/v1/reports/heartbeat", url))
                    .header("Content-Type", "application/json")
                    .header("Authorization", format!("Bearer {}", control_token))
                    .body(http_body_util::Full::new(bytes::Bytes::from(body)))
                    .unwrap();
                let _ = client.request(req).await;
                tracing::debug!("heartbeat sent");
            }
        });
    }

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
