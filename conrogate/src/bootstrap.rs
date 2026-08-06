//! Bootstrap 装配：将所有组件组装并启动。
//!
//! 合并模式装配流程。

use conrogate_core::contract::storage::EventRepo;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 启动全部组件，返回停机信号发送端
pub async fn run(
    config: conrogate_core::contract::config::Config,
) -> anyhow::Result<tokio::sync::broadcast::Sender<()>> {
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    // ── 2. 初始化 DB 连接池 ──
    let main_db = conrogate_core::storage::pool::create_main_pool(&config.db).await?;
    let read_db = conrogate_core::storage::pool::create_read_pool(&config.db).await?;
    let main_db = Arc::new(main_db);
    let read_db = Arc::new(read_db);

    // ── 3. 初始化仓储 ──
    let route_repo =
        Arc::new(conrogate_core::storage::repository::route_repo::RouteRepoImpl::new((*main_db).clone()));
    let upstream_repo = Arc::new(
        conrogate_core::storage::repository::upstream_repo::UpstreamRepoImpl::new((*main_db).clone()),
    );
    let binding_repo = Arc::new(
        conrogate_core::storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new(
            (*main_db).clone(),
        ),
    );
    let config_repo = Arc::new(
        conrogate_core::storage::repository::config_version_repo::ConfigVersionRepoImpl::new(
            (*main_db).clone(),
        ),
    );
    let metric_repo = Arc::new(
        conrogate_core::storage::repository::metric_repo::MetricRepoImpl::new((*main_db).clone()),
    );
    let event_repo =
        Arc::new(conrogate_core::storage::repository::event_repo::EventRepoImpl::new((*main_db).clone()));
    let audit_repo = Arc::new(
        conrogate_core::storage::repository::audit_log_repo::AuditLogRepoImpl::new((*main_db).clone()),
    );
    let node_app_repo = Arc::new(
        conrogate_core::storage::repository::node_application_repo::NodeApplicationRepoImpl::new(
            (*main_db).clone(),
        ),
    );
    let plugin_repo = Arc::new(
        conrogate_core::storage::repository::installed_plugin_repo::InstalledPluginRepoImpl::new(
            (*main_db).clone(),
        ),
    );

    // ── 加载初始配置到内存 ──
    let routes = conrogate_core::contract::storage::ReadOnlyRouteRepo::list_enabled(&*route_repo)
        .await
        .unwrap_or_default();
    let upstreams = conrogate_core::contract::storage::ReadOnlyUpstreamRepo::list_all(&*upstream_repo)
        .await
        .unwrap_or_default();
    // 加载插件绑定（用于 requires_body 静态判定）
    let mut all_bindings = Vec::new();
    for route in &routes {
        let rb = conrogate_core::contract::storage::ReadOnlyPluginBindingRepo::list_by_route(
            &*binding_repo,
            route.id,
        )
        .await
        .unwrap_or_default();
        all_bindings.extend(rb);
    }

    // ── 4. BalancerRegistry ──
    let balancer_registry = conrogate_core::balancer::registry::create_default_registry();

    // ── 5. PassiveHealthChecker ──
    let health_checker = Arc::new(conrogate_gateway::health::PassiveHealthChecker::default());

    // ── 6. StaticDiscovery ──
    let discovery = Arc::new(conrogate_gateway::discovery::StaticDiscovery::new());
    discovery.load(upstreams.clone());

    // ── 7. UpstreamSelector（集成被动健康检查）──
    let upstream_selector = Arc::new(
        conrogate_gateway::pool::UpstreamSelectorImpl::new(balancer_registry)
            .with_health_checker(health_checker.clone()),
    );
    upstream_selector.load_upstreams(upstreams.clone());

    // ── 7a. ActiveHealthChecker（主动健康探测）──
    let active_health_checker =
        Arc::new(conrogate_gateway::health_check::ActiveHealthChecker::default());
    active_health_checker
        .clone()
        .spawn_periodic_check(upstream_selector.shared_upstreams());

    // ── 8. 限流器 / 熔断器 ──
    let limiter = if let Some(ref cluster) = config.gate.rate_limit.cluster_store {
        tracing::info!(redis_url = %cluster.redis_url, "rate limiter: cluster mode (Redis)");
        Arc::new(
            conrogate_core::traffic::limiter::TokenBucketLimiter::new().with_redis(&cluster.redis_url),
        )
    } else {
        Arc::new(conrogate_core::traffic::limiter::TokenBucketLimiter::new())
    };
    let breaker_config = conrogate_core::traffic::breaker::BreakerConfig {
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
    let breaker_factory = Arc::new(conrogate_core::traffic::breaker::BreakerFactoryImpl::new(
        breaker_config,
    ));

    // ── 9. TrafficControl（使用配置中的 QPS 阈值 + 被动健康检查）──
    let traffic = Arc::new(
        conrogate_gateway::filter::TrafficControlAdapter::with_governance_config(
            limiter,
            breaker_factory,
            &config.gate.rate_limit,
            &config.gate.breaker,
        )
        .with_health_checker(health_checker.clone()),
    );

    // ── 10. PluginRegistry + 注册静态插件 ──
    let plugin_registry = Arc::new(conrogate_core::plugin::registry::PluginRegistryImpl::new());
    let log_plugin: Arc<dyn conrogate_core::contract::plugin::Plugin> =
        Arc::new(conrogate_plugin_log::LogPlugin::new());
    let cors_plugin: Arc<dyn conrogate_core::contract::plugin::Plugin> =
        Arc::new(conrogate_plugin_cors::CorsPlugin::new());
    let auth_plugin: Arc<dyn conrogate_core::contract::plugin::Plugin> =
        Arc::new(conrogate_plugin_auth::AuthPlugin::new());
    plugin_registry.register(log_plugin.clone()).await;
    plugin_registry.register(cors_plugin.clone()).await;
    plugin_registry.register(auth_plugin.clone()).await;
    // 调用插件 init() 生命周期钩子
    for p in
        [&*log_plugin, &*cors_plugin, &*auth_plugin] as [&dyn conrogate_core::contract::plugin::Plugin; 3]
    {
        if let Err(e) = p.init(&serde_json::Value::Null).await {
            if p.is_blocking() {
                tracing::error!(plugin = p.name(), error = %e, "blocking plugin init failed, skipping registration");
            } else {
                tracing::warn!(plugin = p.name(), error = %e, "non-blocking plugin init failed, disabled");
            }
        }
    }

    // ── 11. PluginPipeline ──
    let plugin_executor = Arc::new(conrogate_core::plugin::pipeline::PluginPipelineImpl::new());

    // ── 12. RouteMatcher ──
    let route_matcher = Arc::new(conrogate_gateway::route::RouteMatcher::new());
    let body_required = plugin_registry.body_required_plugin_names();
    route_matcher.load_with_bindings(routes, all_bindings, &body_required);

    // ── 13. TelemetryReport ──
    let (metric_tx, metric_rx) = mpsc::channel(100_000);
    let (event_tx, event_rx) = mpsc::channel(100_000);
    let telemetry = Arc::new(conrogate_gateway::telemetry::TelemetryReportImpl::new(
        metric_tx, event_tx,
    ));

    // ── 14. ServiceContext ──
    let svc = Arc::new(conrogate_core::contract::gateway::ServiceContext {
        routes: route_matcher.clone(),
        balancer: upstream_selector.clone(),
        traffic,
        telemetry,
        plugins: plugin_executor.clone(),
        gate_id: config.gate.gate_id.clone(),
    });

    // ── 15-16. 启动数据面（带优雅停机）──
    let gate_config = config.gate.clone();
    let mut gate_shutdown_rx = shutdown_tx.subscribe();
    let gate_route_matcher = route_matcher.clone();
    let gate_upstream_selector = upstream_selector.clone();
    let gate_plugin_registry = plugin_registry.clone();
    let gate_plugin_executor = plugin_executor.clone();
    let gate_handle = tokio::spawn(async move {
        let server = conrogate_gateway::server::GatewayServer::from_components(
            conrogate_core::contract::config::Config {
                gate: gate_config.clone(),
                ..conrogate_core::contract::config::Config::default()
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

    // ── 17. 启动控制面 ──
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

    // ── 18a. 配置热加载后台任务 ──
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

    // ── 18. 后台任务（TaskManager 逆序取消）──
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
    route_repo: Arc<dyn conrogate_core::contract::storage::RouteRepo>,
    upstream_repo: Arc<dyn conrogate_core::contract::storage::UpstreamRepo>,
    binding_repo: Arc<dyn conrogate_core::contract::storage::PluginBindingRepo>,
    config_repo: Arc<dyn conrogate_core::contract::storage::ConfigVersionRepo>,
    metric_repo: Arc<dyn conrogate_core::contract::storage::MetricRepo>,
    event_repo: Arc<dyn conrogate_core::contract::storage::EventRepo>,
    audit_repo: Arc<dyn conrogate_core::contract::storage::AuditLogRepo>,
    node_app_repo: Arc<dyn conrogate_core::contract::storage::NodeApplicationRepo>,
    plugin_repo: Arc<dyn conrogate_core::contract::storage::InstalledPluginRepo>,
}

/// 启动控制面 axum 服务
async fn start_control_plane(
    control_config: conrogate_core::contract::config::ControlConfig,
    repos: ControlRepos,
    redis_url: String,
) {
    // Redis 配置缓存（可选）
    let config_cache: Option<Arc<dyn conrogate_core::contract::storage::ConfigCache>> = if !redis_url
        .is_empty()
    {
        match conrogate_core::storage::config_cache::RedisConfigCache::new(&redis_url) {
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

    let app_state = conrogate_control_svc::AppState {
        svc,
        api_prefix: control_config.listen.api_prefix.clone(),
    };
    let router = conrogate_control_svc::build_router(
        app_state,
        &control_config.auth.token,
        &control_config.listen.api_prefix,
    );

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

/// 配置热加载循环（复用 from_config_with_db 逻辑）
///
/// - 优先从 Redis ConfigCache 读取配置快照（含 Pub/Sub 推送通知）
/// - Redis 不可用时降级为直连 DB 轮询
/// - 原子更新 route_matcher / upstream_selector / plugin_executor
async fn config_hot_reload_loop(
    db: Arc<sea_orm::DatabaseConnection>,
    matcher: Arc<conrogate_gateway::route::RouteMatcher>,
    selector: Arc<conrogate_gateway::pool::UpstreamSelectorImpl>,
    registry: Arc<conrogate_core::plugin::registry::PluginRegistryImpl>,
    plugin_executor: Arc<conrogate_core::plugin::pipeline::PluginPipelineImpl>,
    redis_url: String,
    poll_interval: std::time::Duration,
) {
    // 尝试创建 Redis 配置缓存
    let config_cache: Option<Arc<dyn conrogate_core::contract::storage::ConfigCache>> = if !redis_url
        .is_empty()
    {
        match conrogate_core::storage::config_cache::RedisConfigCache::new(&redis_url) {
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

        // 读取配置：优先 Redis 快照，失败降级直连 DB。任一数据源读取失败
        // 则跳过本次重载，保持当前生效配置（原子替换，不半套刷入）。
        if let Some((r, u, bindings)) = load_config_snapshot(config_cache.as_deref(), &db).await {
            let body_req = registry.body_required_plugin_names();
            // 热加载：构建每绑定独立配置实例的插件链，原子替换插件链缓存。
            // 任一绑定实例化失败则跳过本次重载，保持当前生效配置（fail-open）。
            match conrogate_core::plugin::loader::build_chains(&registry, &bindings) {
                Ok(chains) => {
                    plugin_executor.set_route_chains(chains);
                    matcher.load_with_bindings(r, bindings, &body_req);
                    selector.load_upstreams(u);
                    tracing::debug!("config hot-reloaded");
                }
                Err(e) => {
                    tracing::error!(error = %e, "plugin chain build failed, skip reload");
                }
            }
        }
    }
}

/// 原子读取配置快照：优先 Redis 快照，失败降级直连 DB；
/// 任一数据源读取失败返回 `None`，保持当前生效配置。
async fn load_config_snapshot(
    config_cache: Option<&dyn conrogate_core::contract::storage::ConfigCache>,
    db: &Arc<sea_orm::DatabaseConnection>,
) -> Option<(
    Vec<conrogate_core::contract::dto::RouteDto>,
    Vec<conrogate_core::contract::dto::UpstreamDto>,
    Vec<conrogate_core::contract::dto::PluginBindingDto>,
)> {
    if let Some(cache) = config_cache {
        match cache.get_snapshot().await {
            Ok(Some(snap)) => return Some((snap.routes, snap.upstreams, snap.plugin_bindings)),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error = %e, "config snapshot read failed, falling back to DB");
            }
        }
    }

    load_config_from_db(db).await
}

/// 从数据库直接读取配置（降级路径）
async fn load_config_from_db(
    db: &Arc<sea_orm::DatabaseConnection>,
) -> Option<(
    Vec<conrogate_core::contract::dto::RouteDto>,
    Vec<conrogate_core::contract::dto::UpstreamDto>,
    Vec<conrogate_core::contract::dto::PluginBindingDto>,
)> {
    use conrogate_core::contract::storage::*;

    let route_repo = conrogate_core::storage::repository::route_repo::RouteRepoImpl::new((**db).clone());
    let upstream_repo =
        conrogate_core::storage::repository::upstream_repo::UpstreamRepoImpl::new((**db).clone());
    let binding_repo =
        conrogate_core::storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new(
            (**db).clone(),
        );

    let r = match ReadOnlyRouteRepo::list_enabled(&route_repo).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "config reload: route load failed, keeping current config");
            return None;
        }
    };
    let u = match ReadOnlyUpstreamRepo::list_all(&upstream_repo).await {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(error = %e, "config reload: upstream load failed, keeping current config");
            return None;
        }
    };
    let mut bindings = Vec::new();
    for route in &r {
        match ReadOnlyPluginBindingRepo::list_by_route(&binding_repo, route.id).await {
            Ok(rb) => bindings.extend(rb),
            Err(e) => {
                tracing::warn!(error = %e, "config reload: plugin binding load failed, keeping current config");
                return None;
            }
        }
    }
    Some((r, u, bindings))
}
