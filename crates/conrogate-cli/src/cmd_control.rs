//! `conrogate control` — 分离模式控制面。
//!
//! 仅运行控制面（管理 API + 配置落库 + 指标入库 + 审计）。

use std::sync::Arc;

pub fn run(config: conrogate_core::config::Config) -> anyhow::Result<()> {
    tracing::info!(
        host = %config.control.listen.host,
        port = config.control.listen.port,
        "starting conrogate control (control plane only)"
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("tokio runtime build failed: {e}"))?;

    runtime.block_on(async_run(config))
}

async fn async_run(config: conrogate_core::config::Config) -> anyhow::Result<()> {
    // ── 1. DB 连接池（主库读写）──
    let main_db = conrogate_storage::pool::create_main_pool(&config.db).await?;
    let main_db = Arc::new(main_db);

    // ── 2. 初始化仓储 ──
    let route_repo: Arc<dyn conrogate_core::storage::RouteRepo> =
        Arc::new(conrogate_storage::repository::route_repo::RouteRepoImpl::new((*main_db).clone()));
    let upstream_repo: Arc<dyn conrogate_core::storage::UpstreamRepo> = Arc::new(
        conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new((*main_db).clone()),
    );
    let binding_repo: Arc<dyn conrogate_core::storage::PluginBindingRepo> = Arc::new(
        conrogate_storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new(
            (*main_db).clone(),
        ),
    );
    let config_repo: Arc<dyn conrogate_core::storage::ConfigVersionRepo> = Arc::new(
        conrogate_storage::repository::config_version_repo::ConfigVersionRepoImpl::new(
            (*main_db).clone(),
        ),
    );
    let metric_repo: Arc<dyn conrogate_core::storage::MetricRepo> = Arc::new(
        conrogate_storage::repository::metric_repo::MetricRepoImpl::new((*main_db).clone()),
    );
    let event_repo: Arc<dyn conrogate_core::storage::EventRepo> =
        Arc::new(conrogate_storage::repository::event_repo::EventRepoImpl::new((*main_db).clone()));
    let audit_repo: Arc<dyn conrogate_core::storage::AuditLogRepo> = Arc::new(
        conrogate_storage::repository::audit_log_repo::AuditLogRepoImpl::new((*main_db).clone()),
    );
    let node_app_repo: Arc<dyn conrogate_core::storage::NodeApplicationRepo> = Arc::new(
        conrogate_storage::repository::node_application_repo::NodeApplicationRepoImpl::new(
            (*main_db).clone(),
        ),
    );
    let plugin_repo: Arc<dyn conrogate_core::storage::InstalledPluginRepo> = Arc::new(
        conrogate_storage::repository::installed_plugin_repo::InstalledPluginRepoImpl::new(
            (*main_db).clone(),
        ),
    );
    let ip_blacklist_repo: Arc<dyn conrogate_core::storage::IpBlacklistRepo> = Arc::new(
        conrogate_storage::repository::ip_blacklist_repo::IpBlacklistRepoImpl::new(
            (*main_db).clone(),
        ),
    );

    // ── 3. 组装 ControlService ──
    let config_cache: Option<Arc<dyn conrogate_core::storage::ConfigCache>> = if !config
        .gate
        .refresh
        .config_cache_redis_url
        .is_empty()
    {
        match conrogate_storage::config_cache::RedisConfigCache::new(
            &config.gate.refresh.config_cache_redis_url,
        ) {
            Ok(cache) => {
                tracing::info!("Redis config cache enabled");
                Some(Arc::new(cache))
            }
            Err(e) => {
                tracing::warn!(error = %e, "Redis config cache init failed, falling back to no cache");
                None
            }
        }
    } else {
        None
    };

    let svc = Arc::new(
        conrogate_server::ControlService::new(
            route_repo,
            upstream_repo,
            binding_repo,
            config_repo,
            metric_repo,
            event_repo,
            audit_repo,
            node_app_repo,
            plugin_repo,
            ip_blacklist_repo,
        )
        .with_config_cache(config_cache),
    );

    // ── 4. 组装 axum 路由 + 中间件 ──
    let app_state = conrogate_server::AppState {
        svc,
        api_prefix: config.control.listen.api_prefix.clone(),
    };
    let router = conrogate_server::build_router(
        app_state,
        &config.control.auth.token,
        &config.control.listen.api_prefix,
    );

    // ── 5. 启动控制面监听 ──
    let addr = format!(
        "{}:{}",
        config.control.listen.host, config.control.listen.port
    );
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "conrogate control listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("received SIGINT, initiating graceful shutdown");
        })
        .await?;

    tracing::info!("shutdown complete");
    Ok(())
}
