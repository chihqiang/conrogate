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
            // 无 DB 也能启动（等待 HTTP 拉取配置）
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

    // ── 3. BalancerRegistry ──
    let balancer_registry = conrogate_balancer::registry::create_default_registry();

    // ── 4. UpstreamSelector ──
    let upstream_selector = Arc::new(conrogate_gateway::pool::UpstreamSelectorImpl::new(balancer_registry));
    upstream_selector.load_upstreams(upstreams);

    // ── 5. TrafficControl ──
    let limiter = Arc::new(conrogate_traffic::limiter::TokenBucketLimiter::new());
    let breaker_factory = Arc::new(conrogate_traffic::breaker::BreakerFactoryImpl::default());
    let traffic = Arc::new(conrogate_gateway::filter::TrafficControlAdapter {
        limiter,
        breaker_factory,
    });

    // ── 6. PluginRegistry + 注册静态插件 ──
    let plugin_registry = conrogate_plugin::registry::PluginRegistryImpl::new();
    let log_plugin: Arc<dyn conrogate_contract::plugin::Plugin> = Arc::new(conrogate_plugin_log::LogPlugin::new());
    plugin_registry.register(log_plugin).await;

    // ── 7. PluginPipeline ──
    let plugin_executor = Arc::new(conrogate_plugin::pipeline::PluginPipelineImpl::new());

    // ── 8. RouteMatcher ──
    let route_matcher = Arc::new(conrogate_gateway::route::RouteMatcher::new());
    route_matcher.load(routes);

    // ── 9. TelemetryReport（HTTP 上报模式） ──
    let (metric_tx, _metric_rx) = tokio::sync::mpsc::channel(100_000);
    let (event_tx, _event_rx) = tokio::sync::mpsc::channel(100_000);
    let telemetry = Arc::new(conrogate_gateway::telemetry::TelemetryReportImpl::new(
        metric_tx, event_tx,
    ));

    // ── 10. ServiceContext ──
    let _svc = Arc::new(conrogate_contract::gateway::ServiceContext {
        routes: route_matcher.clone(),
        balancer: upstream_selector.clone(),
        traffic,
        telemetry,
        plugins: plugin_executor,
    });

    // ── 11. 启动数据面监听 ──
    let addr = format!("{}:{}", config.gate.listen.host, config.gate.listen.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "conrogate-gate listening");

    // 等待停机信号
    tokio::signal::ctrl_c().await?;
    tracing::info!("received SIGINT, shutting down");

    Ok(())
}

/// 无 DB 模式启动（仅 HTTP 拉取配置）
async fn run_without_db(config: conrogate_contract::config::Config) -> anyhow::Result<()> {
    tracing::info!("starting gate without db (http config mode)");

    let balancer_registry = conrogate_balancer::registry::create_default_registry();
    let upstream_selector = Arc::new(conrogate_gateway::pool::UpstreamSelectorImpl::new(balancer_registry));

    let limiter = Arc::new(conrogate_traffic::limiter::TokenBucketLimiter::new());
    let breaker_factory = Arc::new(conrogate_traffic::breaker::BreakerFactoryImpl::default());
    let traffic = Arc::new(conrogate_gateway::filter::TrafficControlAdapter {
        limiter,
        breaker_factory,
    });

    let plugin_executor = Arc::new(conrogate_plugin::pipeline::PluginPipelineImpl::new());
    let route_matcher = Arc::new(conrogate_gateway::route::RouteMatcher::new());

    let (metric_tx, _) = tokio::sync::mpsc::channel(100_000);
    let (event_tx, _) = tokio::sync::mpsc::channel(100_000);
    let telemetry = Arc::new(conrogate_gateway::telemetry::TelemetryReportImpl::new(
        metric_tx, event_tx,
    ));

    let _svc = Arc::new(conrogate_contract::gateway::ServiceContext {
        routes: route_matcher,
        balancer: upstream_selector,
        traffic,
        telemetry,
        plugins: plugin_executor,
    });

    let addr = format!("{}:{}", config.gate.listen.host, config.gate.listen.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "conrogate-gate listening (no db)");

    tokio::signal::ctrl_c().await?;
    tracing::info!("received SIGINT, shutting down");

    Ok(())
}
