//! Conrogate 数据面专用二进制（分离模式）。
//!
//! 仅运行数据面（路由→插件→负载均衡→转发），不监听控制面端口。
//! 配置来源：Redis 缓存（优先）/ HTTP 从 control 拉取 / 直连 DB 只读。

mod http_config_loader;

use clap::Parser;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "conrogate-gate")]
#[command(about = "Conrogate 数据面专用二进制")]
struct Cli {
    #[arg(long)]
    env_file: Option<String>,
}

fn main() -> anyhow::Result<()> {
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
        config_source = %config.gate.refresh.config_source,
        worker_threads = config.gate.worker_threads,
        "starting conrogate-gate (data plane only)"
    );

    // 按配置 worker_threads 构建 tokio 运行时（0 = 自动取 CPU 核数）
    let mut runtime_builder = tokio::runtime::Builder::new_multi_thread();
    runtime_builder.enable_all();
    if config.gate.worker_threads > 0 {
        runtime_builder.worker_threads(config.gate.worker_threads);
    }
    let runtime = runtime_builder
        .build()
        .map_err(|e| anyhow::anyhow!("tokio runtime build failed: {e}"))?;
    runtime.block_on(run(config))
}

async fn run(config: conrogate_contract::config::Config) -> anyhow::Result<()> {
    // ── 1. 配置源选择：http 模式不直连 DB ──
    if config.gate.refresh.config_source == "http" {
        tracing::info!("config_source=http, using HTTP config loader only");
        return run_without_db(config).await;
    }

    // ── 2. 只读 DB 连接池（config_source=db 时使用）──
    let read_db = match conrogate_storage::pool::create_read_pool(&config.db).await {
        Ok(db) => Arc::new(db),
        Err(e) => {
            tracing::warn!(error = %e, "read db pool failed, starting with empty config");
            return run_without_db(config).await;
        }
    };

    // ── 2. 使用 from_config_with_db 创建 GatewayServer（自动加载初始配置 + 启动热加载）──
    let server =
        conrogate_gateway::server::GatewayServer::from_config_with_db(config.clone(), read_db)
            .await;

    // ── 4a. 节点心跳上报后台任务（分离模式）──
    let control_url = &config.gate.refresh.control_api_url;
    let control_token = config.gate.refresh.control_api_token.clone();
    let control_api_prefix = config.gate.refresh.control_api_prefix.clone();
    let gate_id = config.common.instance_id.clone();
    if !control_url.is_empty() {
        let url = control_url.clone();
        let api_prefix = control_api_prefix.clone();
        tokio::spawn(async move {
            let client =
                hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
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
                    .uri(format!("{}{}/reports/heartbeat", url, api_prefix))
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
    server
        .run()
        .await
        .map_err(|e| anyhow::anyhow!("gateway server error: {e}"))?;

    Ok(())
}

/// 无 DB 模式启动（仅 HTTP 拉取配置 + 定时轮询热加载）
async fn run_without_db(config: conrogate_contract::config::Config) -> anyhow::Result<()> {
    tracing::info!("starting gate without db (http config mode)");

    let control_url = config.gate.refresh.control_api_url.clone();
    let control_token = config.gate.refresh.control_api_token.clone();
    let control_api_prefix = config.gate.refresh.control_api_prefix.clone();
    let poll_interval = config.gate.refresh.config_poll_interval;
    let server = Arc::new(conrogate_gateway::server::GatewayServer::from_config(config).await);

    async fn reload_from_http(
        server: &conrogate_gateway::server::GatewayServer,
        loader: &http_config_loader::HttpConfigLoader,
    ) {
        // 原子加载：路由/上游/插件绑定任一失败则整体放弃，保持当前配置，
        // 避免瞬时故障被 unwrap_or_default 静默清空配置导致流量中断
        let result = async {
            let routes = loader.load_routes().await?;
            let upstreams = loader.load_upstreams().await?;
            let bindings = loader.load_all_plugin_bindings(&routes).await?;
            Ok::<_, conrogate_contract::ConrogateError>((routes, upstreams, bindings))
        }
        .await;

        match result {
            Ok((routes, upstreams, bindings)) => {
                server.reload_routes_with_bindings(routes, bindings);
                server.reload_upstreams(upstreams);
                tracing::debug!("config hot-reloaded from control HTTP API");
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to reload config from HTTP, keeping current config");
            }
        }
    }

    if !control_url.is_empty() {
        let loader =
            http_config_loader::HttpConfigLoader::new(&control_url, &control_api_prefix, &control_token);
        reload_from_http(&server, &loader).await;

        let poll_server = server.clone();
        let poll_prefix = control_api_prefix.clone();
        let poll_token = control_token.clone();
        tokio::spawn(async move {
            let poll_loader =
                http_config_loader::HttpConfigLoader::new(&control_url, &poll_prefix, &poll_token);
            loop {
                tokio::time::sleep(poll_interval).await;
                reload_from_http(&poll_server, &poll_loader).await;
            }
        });
        tracing::info!(
            interval_ms = poll_interval.as_millis(),
            "config polling started (HTTP mode)"
        );
    } else {
        tracing::warn!("control_api_url is empty, no config loaded");
    }

    server
        .run()
        .await
        .map_err(|e| anyhow::anyhow!("gateway server error: {e}"))?;

    Ok(())
}
