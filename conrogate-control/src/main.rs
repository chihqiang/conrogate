//! Conrogate 控制面专用二进制（分离模式）。
//!
//! 仅运行控制面（管理 API + 配置落库 + 指标入库 + 审计）。

use clap::Parser;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "conrogate-control")]
#[command(about = "Conrogate 控制面专用二进制")]
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
        host = %config.control.listen.host,
        port = config.control.listen.port,
        "starting conrogate-control (control plane only)"
    );

    // ── 1. DB 连接池（主库读写）──
    let main_db = conrogate_storage::pool::create_main_pool(&config.db).await?;
    let main_db = Arc::new(main_db);

    // ── 2. 自动迁移 ──
    if config.node.auto_migrate {
        tracing::info!("auto_migrate enabled, running migrations");
        conrogate_storage::migration::run_migrations(&config.db).await?;
    }

    // ── 3. 初始化仓储 ──
    let route_repo: Arc<dyn conrogate_contract::storage::RouteRepo> = Arc::new(
        conrogate_storage::repository::route_repo::RouteRepoImpl::new((*main_db).clone()),
    );
    let upstream_repo: Arc<dyn conrogate_contract::storage::UpstreamRepo> = Arc::new(
        conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new((*main_db).clone()),
    );
    let binding_repo: Arc<dyn conrogate_contract::storage::PluginBindingRepo> = Arc::new(
        conrogate_storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new((*main_db).clone()),
    );
    let config_repo: Arc<dyn conrogate_contract::storage::ConfigVersionRepo> = Arc::new(
        conrogate_storage::repository::config_version_repo::ConfigVersionRepoImpl::new((*main_db).clone()),
    );
    let metric_repo: Arc<dyn conrogate_contract::storage::MetricRepo> = Arc::new(
        conrogate_storage::repository::metric_repo::MetricRepoImpl::new((*main_db).clone()),
    );
    let event_repo: Arc<dyn conrogate_contract::storage::EventRepo> = Arc::new(
        conrogate_storage::repository::event_repo::EventRepoImpl::new((*main_db).clone()),
    );
    let audit_repo: Arc<dyn conrogate_contract::storage::AuditLogRepo> = Arc::new(
        conrogate_storage::repository::audit_log_repo::AuditLogRepoImpl::new((*main_db).clone()),
    );
    let node_app_repo: Arc<dyn conrogate_contract::storage::NodeApplicationRepo> = Arc::new(
        conrogate_storage::repository::node_application_repo::NodeApplicationRepoImpl::new((*main_db).clone()),
    );
    let plugin_repo: Arc<dyn conrogate_contract::storage::InstalledPluginRepo> = Arc::new(
        conrogate_storage::repository::installed_plugin_repo::InstalledPluginRepoImpl::new((*main_db).clone()),
    );

    // ── 4. 组装 ControlService ──
    // Redis 配置缓存（可选）
    let config_cache: Option<Arc<dyn conrogate_contract::storage::ConfigCache>> =
        if !config.gate.refresh.config_cache_redis_url.is_empty() {
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
        conrogate_control_svc::ControlService::new(
            route_repo,
            upstream_repo,
            binding_repo,
            config_repo,
            metric_repo,
            event_repo,
            audit_repo,
            node_app_repo,
            plugin_repo,
        )
        .with_config_cache(config_cache),
    );

    // ── 5. 组装 axum 路由 + 中间件 ──
    let app_state = conrogate_control_svc::AppState { svc };
    let router = conrogate_control_svc::build_router(
        app_state,
        &config.control.auth.token,
    );

    // ── 6. 启动控制面监听 ──
    let addr = format!(
        "{}:{}",
        config.control.listen.host,
        config.control.listen.port
    );
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "conrogate-control listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("received SIGINT, initiating graceful shutdown");
        })
        .await?;

    tracing::info!("shutdown complete");
    Ok(())
}
