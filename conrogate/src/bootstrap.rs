//! Bootstrap 装配：将所有组件组装并启动。
//!
//! 合并模式装配流程。

use conrogate_contract::storage::EventRepo;
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
    let read_db = Arc::new(read_db);

    // ── 3. [auto_migrate] 自动迁移（PG advisory lock 串行化）──
    if config.node.auto_migrate {
        tracing::info!("auto_migrate enabled, running migrations");
        // PG advisory lock 防止多实例并发迁移
        use sea_orm::ConnectionTrait;
        let lock_result = main_db
            .execute_unprepared("SELECT pg_advisory_lock(20260101)")
            .await;
        if lock_result.is_err() {
            tracing::warn!("failed to acquire advisory lock, proceeding anyway");
        }
        let result = conrogate_storage::migration::run_migrations(&config.db).await;
        let _ = main_db
            .execute_unprepared("SELECT pg_advisory_unlock(20260101)")
            .await;
        result?;
    }

    // ── 4. [seed_demo] 演示数据 ──
    if config.node.seed_demo {
        tracing::info!("seed_demo enabled, writing demo data");
        seed_demo_data(&main_db).await?;
    }

    // ── 5. 初始化仓储 ──
    let route_repo =
        Arc::new(conrogate_storage::repository::route_repo::RouteRepoImpl::new((*main_db).clone()));
    let upstream_repo = Arc::new(
        conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new((*main_db).clone()),
    );
    let binding_repo = Arc::new(
        conrogate_storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new(
            (*main_db).clone(),
        ),
    );
    let config_repo = Arc::new(
        conrogate_storage::repository::config_version_repo::ConfigVersionRepoImpl::new(
            (*main_db).clone(),
        ),
    );
    let metric_repo = Arc::new(
        conrogate_storage::repository::metric_repo::MetricRepoImpl::new((*main_db).clone()),
    );
    let event_repo =
        Arc::new(conrogate_storage::repository::event_repo::EventRepoImpl::new((*main_db).clone()));
    let audit_repo = Arc::new(
        conrogate_storage::repository::audit_log_repo::AuditLogRepoImpl::new((*main_db).clone()),
    );
    let node_app_repo = Arc::new(
        conrogate_storage::repository::node_application_repo::NodeApplicationRepoImpl::new(
            (*main_db).clone(),
        ),
    );
    let plugin_repo = Arc::new(
        conrogate_storage::repository::installed_plugin_repo::InstalledPluginRepoImpl::new(
            (*main_db).clone(),
        ),
    );

    // ── 加载初始配置到内存 ──
    let routes = conrogate_contract::storage::ReadOnlyRouteRepo::list_enabled(&*route_repo)
        .await
        .unwrap_or_default();
    let upstreams = conrogate_contract::storage::ReadOnlyUpstreamRepo::list_all(&*upstream_repo)
        .await
        .unwrap_or_default();
    // 加载插件绑定（用于 requires_body 静态判定）
    let mut all_bindings = Vec::new();
    for route in &routes {
        let rb = conrogate_contract::storage::ReadOnlyPluginBindingRepo::list_by_route(
            &*binding_repo,
            route.id,
        )
        .await
        .unwrap_or_default();
        all_bindings.extend(rb);
    }

    // ── 6. BalancerRegistry ──
    let balancer_registry = conrogate_balancer::registry::create_default_registry();

    // ── 7. PassiveHealthChecker ──
    let health_checker = Arc::new(conrogate_gateway::health::PassiveHealthChecker::default());

    // ── 8. StaticDiscovery ──
    let discovery = Arc::new(conrogate_gateway::discovery::StaticDiscovery::new());
    discovery.load(upstreams.clone());

    // ── 9. UpstreamSelector（集成被动健康检查）──
    let upstream_selector = Arc::new(
        conrogate_gateway::pool::UpstreamSelectorImpl::new(balancer_registry)
            .with_health_checker(health_checker.clone()),
    );
    upstream_selector.load_upstreams(upstreams.clone());

    // ── 9a. ActiveHealthChecker（主动健康探测）──
    let active_health_checker =
        Arc::new(conrogate_gateway::health_check::ActiveHealthChecker::default());
    active_health_checker
        .clone()
        .spawn_periodic_check(upstream_selector.shared_upstreams());

    // ── 10. 限流器 / 熔断器 ──
    let limiter = if let Some(ref cluster) = config.gate.rate_limit.cluster_store {
        tracing::info!(redis_url = %cluster.redis_url, "rate limiter: cluster mode (Redis)");
        Arc::new(
            conrogate_traffic::limiter::TokenBucketLimiter::new().with_redis(&cluster.redis_url),
        )
    } else {
        Arc::new(conrogate_traffic::limiter::TokenBucketLimiter::new())
    };
    let breaker_config = conrogate_traffic::breaker::BreakerConfig {
        window: config.gate.breaker.window,
        failure_rate_threshold: config.gate.breaker.failure_rate_threshold,
        min_requests: config.gate.breaker.min_requests,
        wait: config.gate.breaker.wait,
        half_open_max: config.gate.breaker.half_open_max,
        redis_url: config
            .gate
            .breaker
            .cluster_store
            .as_ref()
            .map(|c| c.redis_url.clone()),
    };
    let breaker_factory = Arc::new(conrogate_traffic::breaker::BreakerFactoryImpl::new(
        breaker_config,
    ));

    // ── 11. TrafficControl（使用配置中的 QPS 阈值 + 被动健康检查）──
    let traffic = Arc::new(
        conrogate_gateway::filter::TrafficControlAdapter::with_governance_config(
            limiter,
            breaker_factory,
            &config.gate.rate_limit,
            &config.gate.breaker,
        )
        .with_health_checker(health_checker.clone()),
    );

    // ── 12. PluginRegistry + 注册静态插件 ──
    let plugin_registry = Arc::new(conrogate_plugin::registry::PluginRegistryImpl::new());
    let log_plugin: Arc<dyn conrogate_contract::plugin::Plugin> =
        Arc::new(conrogate_plugin_log::LogPlugin::new());
    let cors_plugin: Arc<dyn conrogate_contract::plugin::Plugin> =
        Arc::new(conrogate_plugin_cors::CorsPlugin::new());
    let auth_plugin: Arc<dyn conrogate_contract::plugin::Plugin> =
        Arc::new(conrogate_plugin_auth::AuthPlugin::new());
    plugin_registry.register(log_plugin.clone()).await;
    plugin_registry.register(cors_plugin.clone()).await;
    plugin_registry.register(auth_plugin.clone()).await;
    // 调用插件 init() 生命周期钩子
    for p in
        [&*log_plugin, &*cors_plugin, &*auth_plugin] as [&dyn conrogate_contract::plugin::Plugin; 3]
    {
        if let Err(e) = p.init(&serde_json::Value::Null).await {
            if p.is_blocking() {
                tracing::error!(plugin = p.name(), error = %e, "blocking plugin init failed, skipping registration");
            } else {
                tracing::warn!(plugin = p.name(), error = %e, "non-blocking plugin init failed, disabled");
            }
        }
    }

    // ── 13. PluginPipeline ──
    let plugin_executor = Arc::new(conrogate_plugin::pipeline::PluginPipelineImpl::new());

    // ── 14. RouteMatcher ──
    let route_matcher = Arc::new(conrogate_gateway::route::RouteMatcher::new());
    let body_required = plugin_registry.body_required_plugin_names();
    route_matcher.load_with_bindings(routes, all_bindings, &body_required);

    // ── 15. TelemetryReport ──
    let (metric_tx, metric_rx) = mpsc::channel(100_000);
    let (event_tx, event_rx) = mpsc::channel(100_000);
    let telemetry = Arc::new(conrogate_gateway::telemetry::TelemetryReportImpl::new(
        metric_tx, event_tx,
    ));

    // ── 16. ServiceContext ──
    let svc = Arc::new(conrogate_contract::gateway::ServiceContext {
        routes: route_matcher.clone(),
        balancer: upstream_selector.clone(),
        traffic,
        telemetry,
        plugins: plugin_executor.clone(),
    });

    // ── 17-18. 启动数据面（带优雅停机）──
    let gate_config = config.gate.clone();
    let mut gate_shutdown_rx = shutdown_tx.subscribe();
    let gate_route_matcher = route_matcher.clone();
    let gate_upstream_selector = upstream_selector.clone();
    let gate_plugin_registry = plugin_registry.clone();
    let gate_plugin_executor = plugin_executor.clone();
    let gate_handle = tokio::spawn(async move {
        let server = conrogate_gateway::server::GatewayServer::from_components(
            conrogate_contract::config::Config {
                gate: gate_config.clone(),
                ..conrogate_contract::config::Config::default()
            },
            svc,
            gate_plugin_registry,
            gate_route_matcher,
            gate_upstream_selector,
            gate_plugin_executor,
        );
        if let Err(e) = server
            .run_with_shutdown(async move {
                let _ = gate_shutdown_rx.recv().await;
            })
            .await
        {
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

    // ── 20a. 配置热加载后台任务 ──
    let mut task_manager = conrogate_gateway::task_manager::TaskManager::new();
    let hot_reload_redis_url = config.gate.refresh.config_cache_redis_url.clone();
    let hot_reload_db = read_db.clone();
    let hot_reload_matcher = route_matcher.clone();
    let hot_reload_selector = upstream_selector.clone();
    let hot_reload_registry = plugin_registry.clone();
    let hot_reload_executor = plugin_executor.clone();
    let hot_reload_poll = config.gate.refresh.config_poll_interval;
    task_manager.spawn("config-hot-reload", async move {
        config_hot_reload_loop(
            hot_reload_db,
            hot_reload_matcher,
            hot_reload_selector,
            hot_reload_registry,
            hot_reload_executor,
            hot_reload_redis_url,
            hot_reload_poll,
        )
        .await;
    });

    // ── 20. 后台任务（TaskManager 逆序取消）──
    let metric_repo_clone = metric_repo.clone();
    let telemetry_bucket_sec = config.gate.telemetry.bucket_sec.max(1);
    let telemetry_flush = config.gate.telemetry.batch_interval;
    task_manager.spawn("metric-aggregator", async move {
        let mut aggregator =
            conrogate_gateway::telemetry::MetricAggregator::new(metric_rx, telemetry_bucket_sec)
                .with_metric_repo(metric_repo_clone);
        aggregator.run(telemetry_flush).await;
    });

    // 事件消费者：批量读取事件通道并落库
    let event_repo_clone = event_repo.clone();
    let event_batch_size = config.gate.telemetry.batch_size.max(1);
    task_manager.spawn("event-consumer", async move {
        let mut rx = event_rx;
        let mut batch = Vec::new();
        let mut flush_timer = tokio::time::interval(config.gate.telemetry.batch_interval);
        flush_timer.tick().await; // 跳过第一次立即触发
        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    batch.push(event);
                    if batch.len() >= event_batch_size {
                        if let Err(e) = event_repo_clone.insert_batch(&batch).await {
                            tracing::warn!(error = %e, "event batch insert failed");
                        }
                        batch.clear();
                    }
                }
                _ = flush_timer.tick() => {
                    if !batch.is_empty() {
                        if let Err(e) = event_repo_clone.insert_batch(&batch).await {
                            tracing::warn!(error = %e, "event batch insert failed");
                        }
                        batch.clear();
                    }
                }
            }
        }
    });
    tracing::info!("background tasks started");

    // 等待停机信号
    let mut shutdown_recv = shutdown_tx.subscribe();
    // 阻塞等待外部停机信号（由 main 发送 shutdown_tx.send()）
    let _ = shutdown_recv.recv().await;
    tracing::info!("bootstrap shutdown signal received");

    // gate 的 run_with_shutdown 已收到信号，进入宽限期（由 server.rs 内部处理）
    // 等待 gate handle 完成（含宽限期 + idle_timeout 自然超时）
    let gate_shutdown_timeout =
        config.gate.shutdown.long_conn_drain + std::time::Duration::from_secs(5); // 宽限期 + 额外缓冲
    let _ = tokio::time::timeout(gate_shutdown_timeout, gate_handle).await;

    // 逆序取消后台任务（带超时）
    task_manager
        .shutdown(std::time::Duration::from_secs(10))
        .await;
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
    let config_cache: Option<Arc<dyn conrogate_contract::storage::ConfigCache>> = if !redis_url
        .is_empty()
    {
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

    let addr = format!(
        "{}:{}",
        control_config.listen.host, control_config.listen.port
    );
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
    use conrogate_contract::balancer::BalancerAlgorithm;
    use conrogate_contract::dto::*;
    use conrogate_contract::protocol::{PathMatch, ProtocolId, RouteMatchConditions};
    use conrogate_contract::storage::*;

    let upstream_repo =
        conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new((**main_db).clone());
    let route_repo =
        conrogate_storage::repository::route_repo::RouteRepoImpl::new((**main_db).clone());

    // 检查是否已有数据
    let existing = ReadOnlyUpstreamRepo::list_all(&upstream_repo)
        .await
        .unwrap_or_default();
    if !existing.is_empty() {
        tracing::info!("demo data already exists, skipping seed");
        return Ok(());
    }

    // 创建 echo 上游（指向内置 echo 服务 127.0.0.1:9090）
    let upstream = upstream_repo
        .create(CreateUpstreamDto {
            name: "echo-upstream".into(),
            algorithm: BalancerAlgorithm::RoundRobin,
            retry_enabled: Some(false),
            nodes: vec![CreateUpstreamNodeDto {
                address: "127.0.0.1:9090".into(),
                weight: Some(1),
                enabled: Some(true),
            }],
        })
        .await?;

    // 创建演示路由
    let _route = route_repo
        .create(CreateRouteDto {
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
        })
        .await?;

    tracing::info!(
        upstream_id = upstream.id,
        "demo data seeded: echo-upstream + demo-route"
    );
    Ok(())
}

/// 配置热加载循环（复用 from_config_with_db 逻辑）
///
/// - 优先从 Redis ConfigCache 读取配置快照（含 Pub/Sub 推送通知）
/// - Redis 不可用时降级为直连 DB 轮询
/// - 原子更新 route_matcher / upstream_selector / plugin_executor
async fn config_hot_reload_loop(
    db: Arc<sea_orm::DatabaseConnection>,
    matcher: Arc<conrogate_gateway::route::RouteMatcher>,
    selector: Arc<conrogate_gateway::pool::UpstreamSelectorImpl>,
    registry: Arc<conrogate_plugin::registry::PluginRegistryImpl>,
    plugin_executor: Arc<conrogate_plugin::pipeline::PluginPipelineImpl>,
    redis_url: String,
    poll_interval: std::time::Duration,
) {
    // 尝试创建 Redis 配置缓存
    let config_cache: Option<Arc<dyn conrogate_contract::storage::ConfigCache>> = if !redis_url
        .is_empty()
    {
        match conrogate_storage::config_cache::RedisConfigCache::new(&redis_url) {
            Ok(cache) => {
                tracing::info!("data plane: Redis config cache enabled for hot-reload");
                Some(Arc::new(cache))
            }
            Err(e) => {
                tracing::warn!(error = %e, "Redis config cache init failed, using poll-only mode");
                None
            }
        }
    } else {
        None
    };

    // 尝试订阅 Redis Pub/Sub 配置变更通知
    let mut sub_rx: Option<tokio::sync::watch::Receiver<u64>> = None;
    if let Some(ref cache) = config_cache {
        match cache.subscribe_changes().await {
            Ok(Some(rx)) => {
                tracing::info!("subscribed to Redis Pub/Sub config change notifications");
                sub_rx = Some(rx);
            }
            Ok(None) => {
                tracing::info!("ConfigCache does not support Pub/Sub, using poll-only mode");
            }
            Err(e) => {
                tracing::warn!(error = %e, "subscribe_changes failed, using poll-only mode");
            }
        }
    }

    let poll_dur = if poll_interval.as_secs() == 0 {
        std::time::Duration::from_secs(10)
    } else {
        poll_interval
    };

    loop {
        // 如果有 Pub/Sub 订阅，等待通知或超时后轮询
        if let Some(ref mut rx) = sub_rx {
            let timeout = tokio::time::sleep(poll_dur);
            tokio::select! {
                _ = rx.changed() => {
                    tracing::debug!("config change notification received, reloading");
                }
                _ = timeout => {
                    // 超时后也做一次轮询（兜底）
                }
            }
        } else {
            // 无 Pub/Sub，纯轮询
            tokio::time::sleep(poll_dur).await;
        }

        // 读取配置：优先 Redis 快照，失败降级直连 DB
        let (r, u, bindings) = if let Some(ref cache) = config_cache {
            match cache.get_snapshot().await {
                Ok(Some(snap)) => (snap.routes, snap.upstreams, snap.plugin_bindings),
                _ => load_config_from_db(&db).await,
            }
        } else {
            load_config_from_db(&db).await
        };

        if !r.is_empty() || !u.is_empty() {
            let body_req = registry.body_required_plugin_names();
            // 热加载：更新路由插件链
            let mut chains: std::collections::HashMap<
                u64,
                Vec<Arc<dyn conrogate_contract::plugin::Plugin>>,
            > = std::collections::HashMap::new();
            for binding in &bindings {
                if !binding.enabled {
                    continue;
                }
                if let Some(plugin) = registry.get(&binding.plugin_name) {
                    chains.entry(binding.route_id).or_default().push(plugin);
                }
            }
            plugin_executor.set_route_chains(chains);
            matcher.load_with_bindings(r, bindings, &body_req);
            selector.load_upstreams(u);
            tracing::debug!("config hot-reloaded");
        }
    }
}

/// 从数据库直接读取配置（降级路径）
async fn load_config_from_db(
    db: &Arc<sea_orm::DatabaseConnection>,
) -> (
    Vec<conrogate_contract::dto::RouteDto>,
    Vec<conrogate_contract::dto::UpstreamDto>,
    Vec<conrogate_contract::dto::PluginBindingDto>,
) {
    use conrogate_contract::storage::*;

    let route_repo = conrogate_storage::repository::route_repo::RouteRepoImpl::new((**db).clone());
    let upstream_repo =
        conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new((**db).clone());
    let binding_repo =
        conrogate_storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new(
            (**db).clone(),
        );

    let r = ReadOnlyRouteRepo::list_enabled(&route_repo)
        .await
        .unwrap_or_default();
    let u = ReadOnlyUpstreamRepo::list_all(&upstream_repo)
        .await
        .unwrap_or_default();
    let mut bindings = Vec::new();
    for route in &r {
        let rb = ReadOnlyPluginBindingRepo::list_by_route(&binding_repo, route.id)
            .await
            .unwrap_or_default();
        bindings.extend(rb);
    }
    (r, u, bindings)
}
