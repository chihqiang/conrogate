//! Bootstrap 装配：将所有组件组装并启动。
//!
//! 合并模式装配流程。

use std::sync::Arc;
use tokio::sync::mpsc;

/// 启动全部组件，返回停机信号发送端
pub async fn run(
    config: conrogate_contract::config::Config,
) -> anyhow::Result<tokio::sync::broadcast::Sender<()>> {
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    // ── 2. 初始化 DB 连接池 ──
    let main_db = conrogate_storage::pool::create_main_pool(&config.db).await?;
    let read_db = conrogate_storage::pool::create_read_pool(&config.db).await?;
    let main_db = Arc::new(main_db);
    let _read_db = Arc::new(read_db);

      // ── 3. [auto_migrate] 自动迁移（PG advisory lock 串行化）──
      if config.node.auto_migrate {
          tracing::info!("auto_migrate enabled, running migrations");
          // PG advisory lock 防止多实例并发迁移
          use sea_orm::ConnectionTrait;
          let lock_result = main_db.execute_unprepared("SELECT pg_advisory_lock(20260101)").await;
          if lock_result.is_err() {
              tracing::warn!("failed to acquire advisory lock, proceeding anyway");
          }
          let result = conrogate_storage::migration::run_migrations(&config.db).await;
          let _ = main_db.execute_unprepared("SELECT pg_advisory_unlock(20260101)").await;
          result?;
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
    // 加载插件绑定（用于 requires_body 静态判定）
    let mut all_bindings = Vec::new();
    for route in &routes {
        let rb = conrogate_contract::storage::ReadOnlyPluginBindingRepo::list_by_route(
            &*binding_repo, route.id,
        ).await.unwrap_or_default();
        all_bindings.extend(rb);
    }

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
    let body_required = plugin_registry.body_required_plugin_names();
    route_matcher.load_with_bindings(routes, all_bindings, &body_required);

    // ── 15. TelemetryReport ──
    let (metric_tx, metric_rx) = mpsc::channel(100_000);
    let (event_tx, _event_rx) = mpsc::channel(100_000);
    let telemetry = Arc::new(conrogate_gateway::telemetry::TelemetryReportImpl::new(
        metric_tx, event_tx,
    ));

    // ── 16. ServiceContext ──
    let _svc = Arc::new(conrogate_contract::gateway::ServiceContext {
        routes: route_matcher.clone(),
        balancer: upstream_selector.clone(),
        traffic,
        telemetry,
        plugins: plugin_executor,
    });

    // ── 17-18. 启动数据面（带优雅停机）──
    let gate_config = config.gate.clone();
    let mut gate_shutdown_rx = shutdown_tx.subscribe();
    let gate_handle = tokio::spawn(async move {
        let server = conrogate_gateway::server::GatewayServer::from_config(
            conrogate_contract::config::Config {
                gate: gate_config.clone(),
                ..conrogate_contract::config::Config::default()
            },
        )
        .await;
        if let Err(e) = server.run_with_shutdown(async move {
            let _ = gate_shutdown_rx.recv().await;
        }).await {
            tracing::error!(error = %e, "gate server error");
        }
    });

    // ── 19. 启动控制面 ──
    if config.control.listen.enabled {
        let _control_db = main_db.clone();
        let control_config = config.control.clone();
        let redis_url = config.gate.refresh.config_cache_redis_url.clone();
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
            start_control_plane(control_config, repos, redis_url).await;
        });
    }

    // ── 20. 后台任务（TaskManager 逆序取消）──
    let mut task_manager = conrogate_gateway::task_manager::TaskManager::new();
    let metric_repo_clone = metric_repo.clone();
    task_manager.spawn("metric-aggregator", async move {
        let mut aggregator = conrogate_gateway::telemetry::MetricAggregator::new(
            metric_rx,
            10,
        ).with_metric_repo(metric_repo_clone);
        aggregator.run(std::time::Duration::from_secs(10)).await;
    });
    tracing::info!("background tasks started");

    // 等待停机信号
    let mut shutdown_recv = shutdown_tx.subscribe();
    // 阻塞等待外部停机信号（由 main 发送 shutdown_tx.send()）
    let _ = shutdown_recv.recv().await;
    tracing::info!("bootstrap shutdown signal received");

    // gate 的 run_with_shutdown 已收到信号，进入宽限期（由 server.rs 内部处理）
    // 等待 gate handle 完成（含宽限期 + idle_timeout 自然超时）
    let gate_shutdown_timeout = config.gate.shutdown.long_conn_drain
        + std::time::Duration::from_secs(5); // 宽限期 + 额外缓冲
    let _ = tokio::time::timeout(gate_shutdown_timeout, gate_handle).await;

    // 逆序取消后台任务（带超时）
    task_manager.shutdown(std::time::Duration::from_secs(10)).await;
    tracing::info!("shutdown complete");

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
    redis_url: String,
) {
    // Redis 配置缓存（可选）
    let config_cache: Option<Arc<dyn conrogate_contract::storage::ConfigCache>> =
        if !redis_url.is_empty() {
            match conrogate_storage::config_cache::RedisConfigCache::new(&redis_url) {
                Ok(cache) => {
                    tracing::info!(redis_url = %redis_url, "control plane: Redis config cache enabled");
                    Some(Arc::new(cache))
                }
                Err(e) => {
                    tracing::warn!(error = %e, "control plane: Redis config cache init failed");
                    None
                }
            }
        } else {
            None
        };

    let svc = Arc::new(
        conrogate_control_svc::ControlService::new(
            repos.route_repo,
            repos.upstream_repo,
            repos.binding_repo,
            repos.config_repo,
            repos.metric_repo,
            repos.event_repo,
            repos.audit_repo,
            repos.node_app_repo,
            repos.plugin_repo,
        )
        .with_config_cache(config_cache),
    );

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

/// 写入演示数据：1 个 echo 上游 + 1 个演示路由
async fn seed_demo_data(main_db: &Arc<sea_orm::DatabaseConnection>) -> anyhow::Result<()> {
    use conrogate_contract::dto::*;
    use conrogate_contract::storage::*;
    use conrogate_contract::protocol::{PathMatch, ProtocolId, RouteMatchConditions};
    use conrogate_contract::balancer::BalancerAlgorithm;

    let upstream_repo = conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new(
        (**main_db).clone(),
    );
    let route_repo = conrogate_storage::repository::route_repo::RouteRepoImpl::new(
        (**main_db).clone(),
    );

    // 检查是否已有数据
    let existing = ReadOnlyUpstreamRepo::list_all(&upstream_repo).await
        .unwrap_or_default();
    if !existing.is_empty() {
        tracing::info!("demo data already exists, skipping seed");
        return Ok(());
    }

    // 创建 echo 上游（指向内置 echo 服务 127.0.0.1:9090）
    let upstream = upstream_repo.create(CreateUpstreamDto {
        name: "echo-upstream".into(),
        algorithm: BalancerAlgorithm::RoundRobin,
        retry_enabled: Some(false),
        nodes: vec![CreateUpstreamNodeDto {
            address: "127.0.0.1:9090".into(),
            weight: Some(1),
            enabled: Some(true),
        }],
    }).await?;

    // 创建演示路由
    let _route = route_repo.create(CreateRouteDto {
        name: "demo-route".into(),
        protocol: ProtocolId::Http,
        match_conditions: RouteMatchConditions {
            path: PathMatch::Prefix("/demo/".into()),
            methods: None,
            host: None,
            headers: vec![],
            query_params: vec![],
        },
        priority: Some(10),
        upstream_id: Some(upstream.id),
        host_header: None,
        allow_retry_non_idempotent: Some(false),
        enabled: Some(true),
    }).await?;

    tracing::info!(upstream_id = upstream.id, "demo data seeded: echo-upstream + demo-route");
    Ok(())
}
