//! Bootstrap 装配：将所有组件组装并启动。
//!
//! 见 docs/01-architecture.md §8 装配流程。

use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// 启动全部组件，返回停机信号发送端
pub async fn run(
    config: conrogate_contract::config::Config,
) -> anyhow::Result<oneshot::Sender<()>> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    // ── 2. 初始化 DB 连接池 ──
    let main_db = conrogate_storage::pool::create_main_pool(&config.db).await?;
    let read_db = conrogate_storage::pool::create_read_pool(&config.db).await?;
    let main_db = Arc::new(main_db);
    let read_db = Arc::new(read_db);

    // ── 3. [auto_migrate] 自动迁移 ──
    if config.node.auto_migrate {
        tracing::info!("auto_migrate enabled, running migrations");
        conrogate_storage::migration::run_migrations(&config.db).await?;
    }

    // ── 4. [seed_demo] 演示数据 ──
    if config.node.seed_demo {
        tracing::info!("seed_demo enabled, writing demo data");
        seed_demo_data(&main_db).await?;
    }

    // ── 5. 初始化仓储 ──
    let route_repo = Arc::new(conrogate_storage::repository::route_repo::RouteRepoImpl::new(
        (*main_db).clone(),
    ));
    let upstream_repo = Arc::new(conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new(
        (*main_db).clone(),
    ));
    let binding_repo = Arc::new(conrogate_storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new(
        (*main_db).clone(),
    ));
    let config_repo = Arc::new(conrogate_storage::repository::config_version_repo::ConfigVersionRepoImpl::new(
        (*main_db).clone(),
    ));
    let metric_repo = Arc::new(conrogate_storage::repository::metric_repo::MetricRepoImpl::new(
        (*main_db).clone(),
    ));
    let event_repo = Arc::new(conrogate_storage::repository::event_repo::EventRepoImpl::new(
        (*main_db).clone(),
    ));
    let audit_repo = Arc::new(conrogate_storage::repository::audit_log_repo::AuditLogRepoImpl::new(
        (*main_db).clone(),
    ));
    let node_app_repo = Arc::new(conrogate_storage::repository::node_application_repo::NodeApplicationRepoImpl::new(
        (*main_db).clone(),
    ));
    let plugin_repo = Arc::new(conrogate_storage::repository::installed_plugin_repo::InstalledPluginRepoImpl::new(
        (*main_db).clone(),
    ));

    // ── 加载初始配置到内存 ──
    let routes = conrogate_contract::storage::ReadOnlyRouteRepo::list_enabled(&*route_repo).await
        .unwrap_or_default();
    let upstreams = conrogate_contract::storage::ReadOnlyUpstreamRepo::list_all(&*upstream_repo).await
        .unwrap_or_default();

    // ── 6. BalancerRegistry ──
    let balancer_registry = conrogate_balancer::registry::create_default_registry();

    // ── 7. PassiveHealthChecker ──
    let _health_checker = Arc::new(conrogate_gateway::health::PassiveHealthChecker::default());

    // ── 8. StaticDiscovery ──
    let discovery = Arc::new(conrogate_gateway::discovery::StaticDiscovery::new());
    discovery.load(upstreams.clone());

    // ── 9. UpstreamSelector ──
    let upstream_selector = Arc::new(conrogate_gateway::pool::UpstreamSelectorImpl::new(balancer_registry));
    upstream_selector.load_upstreams(upstreams.clone());

    // ── 10. 限流器 / 熔断器 ──
    let limiter = Arc::new(conrogate_traffic::limiter::TokenBucketLimiter::new());
    let breaker_factory = Arc::new(conrogate_traffic::breaker::BreakerFactoryImpl::default());

    // ── 11. TrafficControl ──
    let traffic = Arc::new(conrogate_gateway::filter::TrafficControlAdapter {
        limiter,
        breaker_factory,
    });

    // ── 12. PluginRegistry + 注册静态插件 ──
    let plugin_registry = conrogate_plugin::registry::PluginRegistryImpl::new();
    let log_plugin: Arc<dyn conrogate_contract::plugin::Plugin> = Arc::new(conrogate_plugin_log::LogPlugin::new());
    plugin_registry.register(log_plugin.clone()).await;

    // ── 13. PluginPipeline ──
    let plugin_executor = Arc::new(conrogate_plugin::pipeline::PluginPipelineImpl::new());

    // ── 14. RouteMatcher ──
    let route_matcher = Arc::new(conrogate_gateway::route::RouteMatcher::new());
    route_matcher.load(routes);

    // ── 15. TelemetryReport ──
    let (metric_tx, metric_rx) = mpsc::channel(100_000);
    let (event_tx, _event_rx) = mpsc::channel(100_000);
    let telemetry = Arc::new(conrogate_gateway::telemetry::TelemetryReportImpl::new(
        metric_tx, event_tx,
    ));

    // ── 16. ServiceContext ──
    let svc = Arc::new(conrogate_contract::gateway::ServiceContext {
        routes: route_matcher.clone(),
        balancer: upstream_selector.clone(),
        traffic,
        telemetry,
        plugins: plugin_executor,
    });

    // ── 17-18. 启动数据面 ──
    let gate_config = config.gate.clone();
    let gate_svc = svc.clone();
    let gate_handle = tokio::spawn(async move {
        let server = conrogate_gateway::server::GatewayServer::from_config(
            conrogate_contract::config::Config {
                gate: gate_config,
                ..conrogate_contract::config::Config::default()
            },
        );
        // 注意：GatewayServer::from_config 内部会创建自己的 ServiceContext，
        // 实际生产中应注入外部 svc，此处简化为独立构建
        if let Err(e) = server.run().await {
            tracing::error!(error = %e, "gate server error");
        }
    });

    // ── 19. 启动控制面 ──
    if config.control.listen.enabled {
        let control_db = main_db.clone();
        let control_config = config.control.clone();
        let repos = ControlRepos {
            route_repo: route_repo.clone(),
            upstream_repo: upstream_repo.clone(),
            binding_repo: binding_repo.clone(),
            config_repo: config_repo.clone(),
            metric_repo: metric_repo.clone(),
            event_repo: event_repo.clone(),
            audit_repo: audit_repo.clone(),
            node_app_repo: node_app_repo.clone(),
            plugin_repo: plugin_repo.clone(),
        };
        let _control_handle = tokio::spawn(async move {
            start_control_plane(control_config, repos).await;
        });
    }

    // ── 20. 后台任务 ──
    let _metric_agg = tokio::spawn(async move {
        let mut aggregator = conrogate_gateway::telemetry::MetricAggregator::new(
            metric_rx,
            10,
        );
        aggregator.run(std::time::Duration::from_secs(10)).await;
    });

    // 等待停机信号
    let _ = shutdown_rx.await;
    gate_handle.abort();

    Ok(shutdown_tx)
}

/// 控制面仓储聚合
struct ControlRepos {
    route_repo: Arc<dyn conrogate_contract::storage::RouteRepo>,
    upstream_repo: Arc<dyn conrogate_contract::storage::UpstreamRepo>,
    binding_repo: Arc<dyn conrogate_contract::storage::PluginBindingRepo>,
    config_repo: Arc<dyn conrogate_contract::storage::ConfigVersionRepo>,
    metric_repo: Arc<dyn conrogate_contract::storage::MetricRepo>,
    event_repo: Arc<dyn conrogate_contract::storage::EventRepo>,
    audit_repo: Arc<dyn conrogate_contract::storage::AuditLogRepo>,
    node_app_repo: Arc<dyn conrogate_contract::storage::NodeApplicationRepo>,
    plugin_repo: Arc<dyn conrogate_contract::storage::InstalledPluginRepo>,
}

/// 启动控制面 axum 服务
async fn start_control_plane(
    control_config: conrogate_contract::config::ControlConfig,
    repos: ControlRepos,
) {
    let svc = Arc::new(conrogate_control_svc::ControlService::new(
        repos.route_repo,
        repos.upstream_repo,
        repos.binding_repo,
        repos.config_repo,
        repos.metric_repo,
        repos.event_repo,
        repos.audit_repo,
        repos.node_app_repo,
        repos.plugin_repo,
    ));

    let app_state = conrogate_control_svc::AppState { svc };
    let router = conrogate_control_svc::build_router(app_state, &control_config.auth.token);

    let addr = format!("{}:{}", control_config.listen.host, control_config.listen.port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, addr = %addr, "control plane bind failed");
            return;
        }
    };

    tracing::info!(addr = %addr, "control plane started");
    if let Err(e) = axum::serve(listener, router).await {
        tracing::error!(error = %e, "control plane error");
    }
}

/// 写入演示数据
async fn seed_demo_data(_db: &Arc<sea_orm::DatabaseConnection>) -> anyhow::Result<()> {
    tracing::info!("seed demo data not yet implemented");
    Ok(())
}
