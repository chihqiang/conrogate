//! Bootstrap 装配：将所有组件组装并启动。
//!
//! 见 docs/01-architecture.md §8 装配流程。

use tokio::sync::oneshot;

/// 启动全部组件，返回停机信号发送端
pub async fn run(
    config: conrogate_contract::config::Config,
) -> anyhow::Result<oneshot::Sender<()>> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    // ── 1. Config 已在 main 中加载校验 ──

    // ── 2. 初始化 DB 连接池 ──
    // TODO: 初始化 SeaORM DbConn（只读池 + 主库池）

    // ── 3. [auto_migrate] 自动迁移 ──
    if config.node.auto_migrate {
        tracing::info!("auto_migrate enabled, running migrations");
        // TODO: conrogate_storage::migration::run_migrations(&config.db).await?;
    }

    // ── 4. [seed_demo] 演示数据 ──
    if config.node.seed_demo {
        tracing::info!("seed_demo enabled, writing demo data");
        // TODO: seed 演示路由 + 上游 + 插件绑定
    }

    // ── 5. 初始化仓储 ──
    // TODO: 初始化全部 ReadOnly*Repo + 读写 Repo

    // ── 6. 初始化 BalancerRegistry → 注册 4 种算法 ──
    // TODO

    // ── 7. HealthChecker（PassiveHealthChecker）──
    // TODO

    // ── 8. ServiceDiscovery（StaticDiscovery）──
    // TODO

    // ── 9. UpstreamSelector ──
    // TODO

    // ── 10. 限流器 / 熔断器工厂 ──
    // TODO

    // ── 11. TrafficControl ──
    // TODO

    // ── 12. PluginRegistry → 注册静态插件 ──
    // TODO: registry.register(Arc::new(LogPlugin::new())).await?;
    // TODO: registry.register(Arc::new(CorsPlugin::new())).await?;
    // TODO: registry.register(Arc::new(AuthPlugin::new())).await?;

    // ── 13. PluginPipeline ──
    // TODO

    // ── 14. RouteLookup（从 ConfigLoader 加载 ConfigSnapshot）──
    // TODO

    // ── 15. TelemetryReport（进程内通道 mpsc）──
    // TODO

    // ── 16. ServiceContext 组装 ──
    // TODO

    // ── 17. ProtocolHandlerRegistry ──
    // TODO: register HttpProtocolHandler + TcpTunnelProtocolHandler

    // ── 18. 启动数据面监听 ──
    tracing::info!(
        addr = format!("{}:{}", config.gate.listen.host, config.gate.listen.port),
        "data plane listening"
    );
    // TODO: 绑定数据面端口

    // ── 19. 启动控制面监听 ──
    if config.control.listen.enabled {
        tracing::info!(
            addr = format!("{}:{}", config.control.listen.host, config.control.listen.port),
            "control plane listening"
        );
        // TODO: 绑定控制面端口（axum）
    }

    // ── 20. 启动后台任务 ──
    // TODO: 配置热加载、指标聚合、连接池清扫、心跳

    // 等待停机信号
    tokio::spawn(async move {
        let _ = shutdown_rx.await;
        tracing::info!("bootstrap shutdown signal received");
        // TODO: 按逆序停止后台任务
    });

    Ok(shutdown_tx)
}
