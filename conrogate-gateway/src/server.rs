//! 网关服务入口：启动 HTTP/TCP 监听 + 组装 ServiceContext。

use crate::filter::ConfigReloader;
use crate::pool::UpstreamSelectorImpl;
use crate::route::RouteMatcher;
use crate::telemetry::{MetricAggregator, TelemetryReportImpl};
use bytes::Bytes;
use conrogate_balancer::registry::create_default_registry;
use conrogate_contract::config::Config;
use conrogate_contract::gateway::ServiceContext;
use conrogate_contract::protocol::{ProtocolId, RouteMatchInfo};
use conrogate_contract::storage::EventRepo;
use conrogate_contract::ConrogateError;
use conrogate_plugin::pipeline::PluginPipelineImpl;
use conrogate_plugin::registry::PluginRegistryImpl;
use conrogate_protocol::proxy::ReqBody;
use conrogate_protocol::{
    HttpProtocolHandler, ProtocolHandler, ProtocolHandlerRegistry, TcpTunnelProtocolHandler,
};
use conrogate_traffic::breaker::{BreakerConfig, BreakerFactoryImpl};
use conrogate_traffic::limiter::TokenBucketLimiter;
use http::{Request, Response};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// 网关服务
pub struct GatewayServer {
    config: ConfigReloader,
    route_matcher: Arc<RouteMatcher>,
    upstream_selector: Arc<UpstreamSelectorImpl>,
    protocols: Arc<ProtocolHandlerRegistry>,
    plugin_registry: Arc<PluginRegistryImpl>,
    plugin_executor: Arc<PluginPipelineImpl>,
    max_connections: usize,
    max_body_bytes: usize,
    max_header_bytes: usize,
    idle_timeout: std::time::Duration,
    config_cache: Option<Arc<dyn conrogate_contract::storage::ConfigCache>>,
}

/// 遥测通道（指标/事件接收端），由调用方决定消费方式（DB 落库或日志兜底）
struct TelemetryChannels {
    metric_rx: mpsc::Receiver<conrogate_contract::dto::MetricRow>,
    event_rx: mpsc::Receiver<conrogate_contract::dto::EventRow>,
}

impl GatewayServer {
    /// 组装服务器（不消费遥测通道，由调用方决定消费方式）
    async fn from_config_inner(config: Config) -> (Self, TelemetryChannels) {
        let config_reloader = ConfigReloader::new(config.clone());

        // 路由匹配引擎
        let route_matcher = Arc::new(RouteMatcher::new());

        // 上游选择器
        let registry = create_default_registry();
        let upstream_selector = Arc::new(UpstreamSelectorImpl::new(registry));

        // 流量治理（使用配置中的 QPS 阈值 + Redis 集群限流）
        let limiter = if let Some(ref cluster) = config.gate.rate_limit.cluster_store {
            tracing::info!(redis_url = %cluster.redis_url, "rate limiter: cluster mode (Redis)");
            Arc::new(TokenBucketLimiter::new().with_redis(&cluster.redis_url))
        } else {
            Arc::new(TokenBucketLimiter::new())
        };
        let breaker_config = BreakerConfig {
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
        let breaker_factory = Arc::new(BreakerFactoryImpl::new(breaker_config));
        let traffic = Arc::new(
            crate::filter::TrafficControlAdapter::with_governance_config(
                limiter,
                breaker_factory,
                &config.gate.rate_limit,
                &config.gate.breaker,
            ),
        );

        // 遥测
        let (metric_tx, metric_rx) = mpsc::channel(100_000);
        let (event_tx, event_rx) = mpsc::channel(100_000);
        let telemetry = Arc::new(TelemetryReportImpl::new(metric_tx, event_tx));

        // 插件执行器
        let plugin_executor = Arc::new(PluginPipelineImpl::new());

        // 插件注册表 + 注册内置插件
        let plugin_registry = Arc::new(PluginRegistryImpl::new());
        let log_plugin = Arc::new(conrogate_plugin_log::LogPlugin::new());
        let cors_plugin = Arc::new(conrogate_plugin_cors::CorsPlugin::new());
        let auth_plugin = Arc::new(conrogate_plugin_auth::AuthPlugin::new());
        plugin_registry.register(log_plugin.clone()).await;
        plugin_registry.register(cors_plugin.clone()).await;
        plugin_registry.register(auth_plugin.clone()).await;
        // 调用插件 init() 生命周期钩子
        for p in [&*log_plugin, &*cors_plugin, &*auth_plugin]
            as [&dyn conrogate_contract::plugin::Plugin; 3]
        {
            if let Err(e) = p.init(&serde_json::Value::Null).await {
                if p.is_blocking() {
                    tracing::error!(plugin = p.name(), error = %e, "blocking plugin init failed, skipping registration");
                } else {
                    tracing::warn!(plugin = p.name(), error = %e, "non-blocking plugin init failed, disabled");
                }
            }
        }

        let svc = Arc::new(ServiceContext {
            routes: route_matcher.clone(),
            balancer: upstream_selector.clone(),
            traffic,
            telemetry,
            plugins: plugin_executor.clone(),
            gate_id: config.gate.gate_id.clone(),
        });

        let timeout =
            std::time::Duration::from_millis(config.gate.timeouts.total.as_millis() as u64);

        let rate_limit_enabled = config.gate.rate_limit.enabled;
        let conn_qps = if rate_limit_enabled {
            config.gate.rate_limit.conn_qps
        } else {
            0
        };
        let bandwidth_kbps = if rate_limit_enabled {
            config.gate.rate_limit.bandwidth_kbps
        } else {
            0
        };

        let protocols = ProtocolHandlerRegistry::new();
        protocols.register(Arc::new(
            HttpProtocolHandler::with_registry(svc.clone(), plugin_registry.clone(), timeout)
                .with_outbound_tls(config.gate.outbound_tls.skip_verify)
                .with_trusted_proxies(config.gate.listen.trusted_proxies.clone())
                .with_max_retries(config.gate.retry.max_attempts),
        ));
        protocols.register(Arc::new(TcpTunnelProtocolHandler::with_config(
            svc,
            timeout,
            conn_qps,
            bandwidth_kbps,
        )));

        let server = Self {
            config: config_reloader,
            route_matcher,
            upstream_selector,
            protocols: Arc::new(protocols),
            plugin_registry,
            plugin_executor,
            max_connections: config.gate.connection.max_connections,
            max_body_bytes: config.gate.connection.max_body_bytes,
            max_header_bytes: config.gate.connection.max_header_bytes,
            idle_timeout: config.gate.connection.idle_timeout,
            config_cache: None,
        };
        (server, TelemetryChannels { metric_rx, event_rx })
    }

    /// 从配置构建网关（async：需注册插件）。
    /// 无 DB 场景：遥测仅记录日志（防止通道满后静默丢弃）。
    pub async fn from_config(config: Config) -> Self {
        let (server, channels) = Self::from_config_inner(config).await;
        tokio::spawn(async move {
            let mut rx = channels.metric_rx;
            while let Some(metric) = rx.recv().await {
                tracing::debug!(
                    route_id = ?metric.route_id,
                    qps = metric.qps,
                    latency_ms = metric.avg_latency_ms,
                    "metric received (no DB backend, logging only)"
                );
            }
        });
        tokio::spawn(async move {
            let mut rx = channels.event_rx;
            while let Some(event) = rx.recv().await {
                tracing::debug!(
                    event_type = %event.event_type,
                    route_id = ?event.route_id,
                    "event received (no DB backend, logging only)"
                );
            }
        });
        server
    }

    /// 从已有组件构建网关（bootstrap 装配路径）
    ///
    /// 传入已装配的 `ServiceContext`、`PluginRegistry`、`RouteMatcher` 和 `UpstreamSelectorImpl`，
    /// 确保 server 内部使用的路由表和上游选择器与 bootstrap 装配的是同一实例。
    pub fn from_components(
        config: Config,
        svc: Arc<ServiceContext>,
        plugin_registry: Arc<PluginRegistryImpl>,
        route_matcher: Arc<RouteMatcher>,
        upstream_selector: Arc<UpstreamSelectorImpl>,
        plugin_executor: Arc<PluginPipelineImpl>,
    ) -> Self {
        let config_reloader = ConfigReloader::new(config.clone());
        let timeout =
            std::time::Duration::from_millis(config.gate.timeouts.total.as_millis() as u64);
        let rate_limit_enabled = config.gate.rate_limit.enabled;
        let conn_qps = if rate_limit_enabled {
            config.gate.rate_limit.conn_qps
        } else {
            0
        };
        let bandwidth_kbps = if rate_limit_enabled {
            config.gate.rate_limit.bandwidth_kbps
        } else {
            0
        };

        let protocols = ProtocolHandlerRegistry::new();
        protocols.register(Arc::new(
            HttpProtocolHandler::with_registry(svc.clone(), plugin_registry.clone(), timeout)
                .with_outbound_tls(config.gate.outbound_tls.skip_verify)
                .with_trusted_proxies(config.gate.listen.trusted_proxies.clone())
                .with_max_retries(config.gate.retry.max_attempts),
        ));
        protocols.register(Arc::new(TcpTunnelProtocolHandler::with_config(
            svc,
            timeout,
            conn_qps,
            bandwidth_kbps,
        )));

        Self {
            config: config_reloader,
            route_matcher,
            upstream_selector,
            protocols: Arc::new(protocols),
            plugin_registry,
            plugin_executor,
            max_connections: config.gate.connection.max_connections,
            max_body_bytes: config.gate.connection.max_body_bytes,
            max_header_bytes: config.gate.connection.max_header_bytes,
            idle_timeout: config.gate.connection.idle_timeout,
            config_cache: None,
        }
    }

    /// 从配置 + DB 连接构建网关（含配置热加载）
    pub async fn from_config_with_db(
        config: Config,
        read_db: Arc<conrogate_storage::pool::DbConn>,
    ) -> Self {
        // 提取 Redis 配置（在 config 被 move 之前）
        let redis_url = if !config.gate.refresh.config_cache_redis_url.is_empty() {
            Some(config.gate.refresh.config_cache_redis_url.clone())
        } else {
            config
                .gate
                .rate_limit
                .cluster_store
                .as_ref()
                .filter(|s| !s.redis_url.is_empty())
                .map(|s| s.redis_url.clone())
        };
        let poll_interval = config.gate.refresh.config_poll_interval.as_secs().max(1);
        let telemetry_bucket_sec = config.gate.telemetry.bucket_sec.max(1);
        let telemetry_flush = config.gate.telemetry.batch_interval;
        let telemetry_batch_size = config.gate.telemetry.batch_size.max(1);
        let (mut server, channels) = Self::from_config_inner(config).await;

        // 数据面遥测落库：指标聚合批量写入 + 事件批量写入（复用合并模式管线）
        let metric_repo = Arc::new(
            conrogate_storage::repository::metric_repo::MetricRepoImpl::new((*read_db).clone()),
        );
        let event_repo = Arc::new(
            conrogate_storage::repository::event_repo::EventRepoImpl::new((*read_db).clone()),
        );
        tokio::spawn(async move {
            let mut aggregator = MetricAggregator::new(channels.metric_rx, telemetry_bucket_sec)
                .with_metric_repo(metric_repo);
            aggregator.run(telemetry_flush).await;
        });
        tokio::spawn(async move {
            let mut rx = channels.event_rx;
            let mut batch = Vec::new();
            let mut flush_timer = tokio::time::interval(telemetry_flush);
            flush_timer.tick().await; // 跳过第一次立即触发
            loop {
                tokio::select! {
                    Some(event) = rx.recv() => {
                        batch.push(event);
                        if batch.len() >= telemetry_batch_size {
                            if let Err(e) = event_repo.insert_batch(&batch).await {
                                tracing::warn!(error = %e, "event batch insert failed");
                            }
                            batch.clear();
                        }
                    }
                    _ = flush_timer.tick() => {
                        if !batch.is_empty() {
                            if let Err(e) = event_repo.insert_batch(&batch).await {
                                tracing::warn!(error = %e, "event batch insert failed");
                            }
                            batch.clear();
                        }
                    }
                }
            }
        });

        // 尝试创建 Redis 配置缓存
        if let Some(ref url) = redis_url {
            match conrogate_storage::config_cache::RedisConfigCache::new(url) {
                Ok(cache) => {
                    tracing::info!("Redis config cache initialized for Pub/Sub");
                    server.config_cache = Some(Arc::new(cache));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Redis config cache init failed, using poll-only mode");
                }
            }
        }

        // 加载初始路由 + 上游 + 插件绑定
        let route_repo =
            conrogate_storage::repository::route_repo::RouteRepoImpl::new((*read_db).clone());
        let upstream_repo =
            conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new((*read_db).clone());
        let binding_repo =
            conrogate_storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new(
                (*read_db).clone(),
            );

        let routes = conrogate_contract::storage::ReadOnlyRouteRepo::list_enabled(&route_repo)
            .await
            .unwrap_or_default();
        let upstreams = conrogate_contract::storage::ReadOnlyUpstreamRepo::list_all(&upstream_repo)
            .await
            .unwrap_or_default();
        let mut all_bindings = Vec::new();
        for route in &routes {
            let rb = conrogate_contract::storage::ReadOnlyPluginBindingRepo::list_by_route(
                &binding_repo,
                route.id,
            )
            .await
            .unwrap_or_default();
            all_bindings.extend(rb);
        }

        let body_required = server.plugin_registry.body_required_plugin_names();
        // 初始加载：预解析路由插件链并缓存到 PluginPipelineImpl
        let mut init_chains: std::collections::HashMap<
            u64,
            Vec<Arc<dyn conrogate_contract::plugin::Plugin>>,
        > = std::collections::HashMap::new();
        for binding in &all_bindings {
            if !binding.enabled {
                continue;
            }
            if let Some(plugin) = server.plugin_registry.get(&binding.plugin_name) {
                init_chains
                    .entry(binding.route_id)
                    .or_default()
                    .push(plugin);
            }
        }
        server.plugin_executor.set_route_chains(init_chains);
        server
            .route_matcher
            .load_with_bindings(routes, all_bindings, &body_required);
        server.upstream_selector.load_upstreams(upstreams);

        // 启动配置热加载后台任务
        let matcher = server.route_matcher.clone();
        let selector = server.upstream_selector.clone();
        let registry = server.plugin_registry.clone();
        let plugin_executor = server.plugin_executor.clone();
        let db = read_db.clone();
        let config_cache = server.config_cache.clone();
        let poll_dur = std::time::Duration::from_secs(poll_interval);
        tokio::spawn(async move {
            // 尝试从 Redis ConfigCache 订阅配置变更通知
            let mut sub_rx: Option<tokio::sync::watch::Receiver<u64>> = None;
            if let Some(ref cache) = config_cache {
                match cache.subscribe_changes().await {
                    Ok(Some(rx)) => {
                        tracing::info!("subscribed to Redis Pub/Sub config change notifications");
                        sub_rx = Some(rx);
                    }
                    Ok(None) => {
                        tracing::info!(
                            "ConfigCache does not support Pub/Sub, using poll-only mode"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "subscribe_changes failed, using poll-only mode");
                    }
                }
            }

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

                // 读取配置：优先 Redis 快照，失败降级直连 DB（fail-open，docs/09 §9）
                let (r, u, bindings) = if let Some(ref cache) = config_cache {
                    match cache.get_snapshot().await {
                        Ok(Some(snap)) => (snap.routes, snap.upstreams, snap.plugin_bindings),
                        _ => {
                            let r = conrogate_contract::storage::ReadOnlyRouteRepo::list_enabled(
                                &conrogate_storage::repository::route_repo::RouteRepoImpl::new(
                                    (*db).clone(),
                                ),
                            )
                            .await
                            .unwrap_or_default();
                            let u = conrogate_contract::storage::ReadOnlyUpstreamRepo::list_all(
                                &conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new((*db).clone()),
                            ).await.unwrap_or_default();
                            let mut bindings = Vec::new();
                            for route in &r {
                                let rb = conrogate_contract::storage::ReadOnlyPluginBindingRepo::list_by_route(
                                    &conrogate_storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new((*db).clone()),
                                    route.id,
                                ).await.unwrap_or_default();
                                bindings.extend(rb);
                            }
                            (r, u, bindings)
                        }
                    }
                } else {
                    let r = conrogate_contract::storage::ReadOnlyRouteRepo::list_enabled(
                        &conrogate_storage::repository::route_repo::RouteRepoImpl::new(
                            (*db).clone(),
                        ),
                    )
                    .await
                    .unwrap_or_default();
                    let u = conrogate_contract::storage::ReadOnlyUpstreamRepo::list_all(
                        &conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new(
                            (*db).clone(),
                        ),
                    )
                    .await
                    .unwrap_or_default();
                    let mut bindings = Vec::new();
                    for route in &r {
                        let rb = conrogate_contract::storage::ReadOnlyPluginBindingRepo::list_by_route(
                            &conrogate_storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new((*db).clone()),
                            route.id,
                        ).await.unwrap_or_default();
                        bindings.extend(rb);
                    }
                    (r, u, bindings)
                };
                if !r.is_empty() || !u.is_empty() {
                    let body_req = registry.body_required_plugin_names();
                    // 热加载：更新路由插件链（set_route_chains）
                    // 按 route_id 分组绑定，解析插件实例，原子替换插件链缓存
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
                    tracing::debug!("config hot-reloaded from DB");
                }
            }
        });

        server
    }

    /// 启动网关服务（带优雅停机）
    ///
    /// `shutdown` 为停机 Future：完成后停止 accept 新连接，
    /// 等待宽限期 `long_conn_drain` 后强制结束。
    pub async fn run_with_shutdown<F>(&self, shutdown: F) -> Result<(), ConrogateError>
    where
        F: std::future::Future<Output = ()>,
    {
        let config = self.config.current();
        let addr = format!("{}:{}", config.gate.listen.host, config.gate.listen.port);
        let addr: SocketAddr = addr
            .parse()
            .map_err(|e| ConrogateError::Init(format!("listen addr parse: {e}")))?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| ConrogateError::Init(format!("tcp bind: {e}")))?;

        // 全局并发连接限制（Semaphore）
        let conn_semaphore = Arc::new(tokio::sync::Semaphore::new(self.max_connections));
        let max_body_bytes = self.max_body_bytes;
        let max_header_bytes = self.max_header_bytes;
        let idle_timeout = self.idle_timeout;
        let read_timeout = config.gate.timeouts.read;
        let upgrade_buffer_size = config.gate.upgrade.buffer_size;
        let ws_connect_timeout = config.gate.timeouts.connect;
        let ws_idle_timeout = config.gate.upgrade.idle_timeout;
        let long_conn_drain = config.gate.shutdown.long_conn_drain;
        // 跟踪所有连接任务：宽限期结束后可强制 abort
        let mut connections = tokio::task::JoinSet::new();
        // WS 隧道停机广播
        let ws_shutdown = Arc::new(tokio::sync::Notify::new());

        // TLS 入站终止
        let tls_enabled = config.gate.listen.tls.enabled;
        let tls_mode = config.gate.listen.tls.mode.clone();
        let tls_acceptor = if tls_enabled && tls_mode == "terminate" {
            match crate::tls::build_tls_acceptor(&config.gate.listen.tls) {
                Ok(a) => {
                    tracing::info!(cert_file = %config.gate.listen.tls.cert_file, "TLS terminate mode enabled");
                    Some(a)
                }
                Err(e) => {
                    tracing::error!(error = %e, "TLS config failed, falling back to plain TCP");
                    None
                }
            }
        } else if tls_enabled && tls_mode == "passthrough" {
            tracing::info!("TLS passthrough mode enabled (raw TCP forwarding, SNI-based routing)");
            None
        } else {
            None
        };

        tracing::info!(addr = %addr, max_connections = self.max_connections, tls = tls_enabled, "gateway server started");

        // 优雅停机：select! 在 accept 和 shutdown 之间竞争
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                // 正常 accept 新连接
                accept_result = listener.accept() => {
                    let (stream, remote_addr) = accept_result
                        .map_err(|e| ConrogateError::Init(format!("tcp accept: {e}")))?;

                    let client_ip = remote_addr.ip().to_string();
                    let http_handler = self.protocols.get(ProtocolId::Http);
                    let tcp_handler = self.protocols.get(ProtocolId::TcpTunnel);
                    let route_matcher = self.route_matcher.clone();
                    let semaphore = conn_semaphore.clone();
                    let tls_acc = tls_acceptor.clone();
                    let listen_addr = addr.to_string();
                    let tls_passthrough = tls_enabled && tls_mode == "passthrough";
                    let ws_shutdown = ws_shutdown.clone();

                    connections.spawn(async move {
                        // 获取并发许可
                        let _permit = match semaphore.acquire().await {
                            Ok(p) => p,
                            Err(_) => {
                                tracing::warn!("connection semaphore closed");
                                return;
                            }
                        };

                        // TLS passthrough 模式：原始 TCP 隧道转发，不终止 TLS
                        if tls_passthrough {
                            // peek ClientHello 提取 SNI，用于按域名路由（docs/10 §2.3）
                            let mut buf = [0u8; 4096];
                            let sni = match stream.peek(&mut buf).await {
                                Ok(n) if n >= 5 => {
                                    conrogate_protocol::tls::extract_sni_from_client_hello(&buf[..n])
                                }
                                _ => None,
                            };
                            if let Some(sni) = &sni {
                                tracing::debug!(sni = %sni, "tls passthrough: sni extracted from client hello");
                            }
                            let result = match tcp_handler {
                                Some(handler) => {
                                    handler.handle_tcp(listen_addr, sni, client_ip, stream).await
                                }
                                None => {
                                    tracing::warn!("TCP tunnel protocol handler not registered");
                                    return;
                                }
                            };
                            if let Err(e) = &result {
                                tracing::debug!(error = %e, "tcp tunnel connection ended");
                            }
                            return;
                        }

                        // HTTP 模式（含 TLS 终止）
                        let http_handler = match http_handler {
                            Some(handler) => handler,
                            None => {
                                tracing::warn!("HTTP protocol handler not registered");
                                return;
                            }
                        };
                        let svc = HyperServiceBridge {
                            handler: http_handler,
                            route_matcher,
                            client_ip,
                            max_body_bytes,
                            read_timeout,
                            upgrade_buffer_size,
                            ws_connect_timeout,
                            ws_idle_timeout,
                            ws_shutdown,
                        };
                        let result = if let Some(acc) = tls_acc {
                            match acc.accept(stream).await {
                                Ok(tls_stream) => {
                                    // ALPN 协商决定 HTTP/2 或 HTTP/1.1（docs/10 §2.1 入站 HTTP/2）
                                    let alpn = tls_stream
                                        .get_ref()
                                        .1
                                        .alpn_protocol()
                                        .map(|p| p.to_vec());
                                    let io = TokioIo::new(tls_stream);
                                    tokio::time::timeout(
                                        idle_timeout,
                                        serve_tls_connection(io, svc, max_header_bytes, read_timeout, alpn),
                                    ).await
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "TLS handshake failed");
                                    return;
                                }
                            }
                        } else {
                            // 明文：auto builder 探测 h2c preface，否则回退 HTTP/1.1
                            let io = TokioIo::new(stream);
                            tokio::time::timeout(
                                idle_timeout,
                                serve_cleartext_connection(io, svc, max_header_bytes, read_timeout),
                            ).await
                        };

                        match result {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => {
                                tracing::debug!(error = %e, "http connection ended");
                            }
                            Err(_) => {
                                tracing::debug!("http connection idle timeout");
                            }
                        }
                    });
                }

                // 优雅停机信号
                _ = &mut shutdown => {
                    tracing::info!("shutdown signal received, stopping accept loop");
                    // 停止接受新连接，进入宽限期
                    break;
                }
            }
        }

        // 宽限期：等待存量连接自然结束
        tracing::info!(
            drain_ms = long_conn_drain.as_millis(),
            "graceful shutdown: draining in-flight connections"
        );
        tokio::time::sleep(long_conn_drain).await;

        // 宽限期结束：通知 WS 隧道关闭，并强制终止仍在执行的连接任务
        tracing::info!("graceful drain period expired, force-closing remaining connections");
        ws_shutdown.notify_waiters();
        connections.shutdown().await;
        tracing::info!("all connections closed");

        Ok(())
    }

    /// 启动网关服务（阻塞，无优雅停机）
    pub async fn run(&self) -> Result<(), ConrogateError> {
        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        self.run_with_shutdown(async move {
            let _ = rx.await;
        })
        .await
    }

    /// 热加载路由
    pub fn reload_routes(&self, routes: Vec<conrogate_contract::dto::RouteDto>) {
        self.route_matcher.load(routes);
        tracing::info!("routes reloaded");
    }

    /// 热加载路由 + 插件绑定（含 requires_body 静态判定）
    pub fn reload_routes_with_bindings(
        &self,
        routes: Vec<conrogate_contract::dto::RouteDto>,
        bindings: Vec<conrogate_contract::dto::PluginBindingDto>,
    ) {
        let body_required = self.plugin_registry.body_required_plugin_names();
        self.route_matcher
            .load_with_bindings(routes, bindings, &body_required);
        tracing::info!("routes reloaded with bindings");
    }

    /// 热加载上游
    pub fn reload_upstreams(&self, upstreams: Vec<conrogate_contract::dto::UpstreamDto>) {
        self.upstream_selector.load_upstreams(upstreams);
        tracing::info!("upstreams reloaded");
    }

    /// 获取插件注册表引用
    pub fn plugin_registry(&self) -> &Arc<PluginRegistryImpl> {
        &self.plugin_registry
    }

    /// 优雅停机：调用所有插件的 shutdown()
    pub async fn shutdown_plugins(&self) {
        let plugins = self.plugin_registry.list_all();
        for p in &plugins {
            if let Err(e) = p.shutdown().await {
                tracing::warn!(plugin = p.name(), error = %e, "plugin shutdown failed");
            }
        }
        tracing::info!("all plugins shut down");
    }
}

/// hyper Service 桥接器
#[derive(Clone)]
struct HyperServiceBridge {
    handler: Arc<dyn ProtocolHandler>,
    route_matcher: Arc<RouteMatcher>,
    client_ip: String,
    max_body_bytes: usize,
    /// 客户端读取超时（慢读保护）：请求体收集阶段
    read_timeout: std::time::Duration,
    upgrade_buffer_size: usize,
    ws_connect_timeout: std::time::Duration,
    ws_idle_timeout: std::time::Duration,
    /// 停机通知：宽限期结束后广播，WS 转发任务据此关闭隧道
    ws_shutdown: Arc<tokio::sync::Notify>,
}

/// WebSocket 升级信息（存入响应扩展）
#[derive(Clone)]
pub struct WsUpgradeInfo {
    pub upstream_addr: String,
    pub trace_id: String,
}

/// 明文连接：auto builder 探测 h2c preface（HTTP/2 prior knowledge），否则回退 HTTP/1.1
async fn serve_cleartext_connection<I, S>(
    io: I,
    svc: S,
    max_header_bytes: usize,
    read_timeout: std::time::Duration,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
    S: hyper::service::Service<
            hyper::Request<Incoming>,
            Response = hyper::Response<ReqBody>,
            Error = ConrogateError,
        > + Send
        + Clone
        + 'static,
    S::Future: Send + 'static,
{
    let mut builder = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new());
    builder.http1().max_buf_size(max_header_bytes);
    // 慢读保护：客户端超时未发完整请求头则断开（HTTP/1.1）
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(Some(read_timeout));
    builder
        .http2()
        .max_header_list_size(max_header_bytes as u32);
    builder.serve_connection(io, svc).await
}

/// TLS 连接：按 ALPN 协商结果选择 HTTP/2（h2）或 HTTP/1.1
async fn serve_tls_connection<I, S>(
    io: I,
    svc: S,
    max_header_bytes: usize,
    read_timeout: std::time::Duration,
    alpn: Option<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
    S: hyper::service::Service<
            hyper::Request<Incoming>,
            Response = hyper::Response<ReqBody>,
            Error = ConrogateError,
        > + Send
        + Clone
        + 'static,
    S::Future: Send + 'static,
{
    if alpn.as_deref() == Some(b"h2") {
        hyper::server::conn::http2::Builder::new(TokioExecutor::new())
            .max_header_list_size(max_header_bytes as u32)
            .serve_connection(io, svc)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    } else {
        let mut h1 = hyper::server::conn::http1::Builder::new();
        h1.max_buf_size(max_header_bytes);
        // 慢读保护：客户端超时未发完整请求头则断开
        h1.timer(TokioTimer::new())
            .header_read_timeout(Some(read_timeout));
        h1.serve_connection(io, svc)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

/// 将 Bytes 包装为统一响应体（ReqBody），兼容 HTTP/1.1 与 HTTP/2
fn boxed_body(bytes: Bytes) -> ReqBody {
    use http_body_util::combinators::BoxBody;
    BoxBody::new(
        Full::new(bytes).map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { match e {} }),
    )
}

/// 构造 JSON 错误响应体
fn json_error(code: i32, msg: &str) -> Bytes {
    let trace_id = format!(
        "{:032x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let body = serde_json::json!({
        "code": code,
        "msg": msg,
        "trace_id": trace_id
    });
    Bytes::from(serde_json::to_vec(&body).unwrap_or_default())
}

/// 构造 JSON 错误响应
fn error_response(status: http::StatusCode, code: i32, msg: &str) -> Response<ReqBody> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(boxed_body(json_error(code, msg)))
        .unwrap()
}

impl hyper::service::Service<Request<Incoming>> for HyperServiceBridge {
    type Response = Response<ReqBody>;
    type Error = ConrogateError;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn call(&self, mut req: Request<Incoming>) -> Self::Future {
        let handler = self.handler.clone();
        let route_matcher = self.route_matcher.clone();
        let client_ip = self.client_ip.clone();
        let max_body_bytes = self.max_body_bytes;
        let read_timeout = self.read_timeout;
        let upgrade_buffer_size = self.upgrade_buffer_size;
        let ws_connect_timeout = self.ws_connect_timeout;
        let ws_idle_timeout = self.ws_idle_timeout;
        let ws_shutdown = self.ws_shutdown.clone();

        Box::pin(async move {
            // 健康探针：GET /healthz → 200
            if req.method() == http::Method::GET && req.uri().path() == "/healthz" {
                return Ok(Response::builder()
                    .status(http::StatusCode::OK)
                    .body(boxed_body(Bytes::from_static(b"ok")))
                    .unwrap());
            }

            // 就绪探针：GET /readyz → 200（路由缓存非空）/ 503（路由为空）
            if req.method() == http::Method::GET && req.uri().path() == "/readyz" {
                if route_matcher.is_empty() {
                    return Ok(error_response(
                        http::StatusCode::SERVICE_UNAVAILABLE,
                        50001,
                        "not ready: no routes loaded",
                    ));
                }
                return Ok(Response::builder()
                    .status(http::StatusCode::OK)
                    .body(boxed_body(Bytes::from_static(b"ready")))
                    .unwrap());
            }

            // WebSocket 升级检测：在拆分请求前提取 OnUpgrade future
            let is_ws_upgrade = req.method() == http::Method::GET
                && req
                    .headers()
                    .get("upgrade")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.eq_ignore_ascii_case("websocket"))
                    .unwrap_or(false);
            let on_upgrade = if is_ws_upgrade {
                Some(hyper::upgrade::on(&mut req))
            } else {
                None
            };
            // 保存 WS 升级请求信息（用于构造转发到上游的握手请求）
            let ws_req_info = if is_ws_upgrade {
                Some((
                    req.method().clone(),
                    req.uri().clone(),
                    req.headers().clone(),
                ))
            } else {
                None
            };

            // 拆分请求：先匹配路由，判定是否需要缓冲 body
            let (parts, body) = req.into_parts();
            let match_info =
                RouteMatchInfo::from_http_request(&parts.method, &parts.uri, &parts.headers);

            // 尝试路由匹配
            let matched_route = route_matcher.match_route(ProtocolId::Http, &match_info);

            // 流式模式：路由命中且无 requires_body 插件 → 不 collect body，直接透传
            if let Some(ref route) = matched_route {
                if !route.requires_body {
                    // 流式模式请求体大小限制（通过 Content-Length 头检查）
                    if let Some(cl) = parts
                        .headers
                        .get("content-length")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<usize>().ok())
                    {
                        if cl > max_body_bytes {
                            return Ok(error_response(
                                http::StatusCode::PAYLOAD_TOO_LARGE,
                                10007,
                                "request body too large",
                            ));
                        }
                    }
                    let resp = match handler
                        .handle_http_stream(parts, body, route.clone(), client_ip)
                        .await
                    {
                        Ok(resp) => resp,
                        Err(ConrogateError::RateLimited) | Err(ConrogateError::Limited) => {
                            return Ok(error_response(
                                http::StatusCode::TOO_MANY_REQUESTS,
                                40008,
                                "rate limited",
                            ));
                        }
                        Err(ConrogateError::CircuitBreakerOpen) => {
                            return Ok(error_response(
                                http::StatusCode::SERVICE_UNAVAILABLE,
                                40007,
                                "circuit breaker open",
                            ));
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "stream handler error");
                            return Ok(error_response(
                                http::StatusCode::BAD_GATEWAY,
                                40006,
                                "gateway error",
                            ));
                        }
                    };
                    // WebSocket 升级响应检测（流式路径）
                    if resp.status() == http::StatusCode::SWITCHING_PROTOCOLS {
                        if let Some(upstream_addr) = resp
                            .headers()
                            .get("X-WS-Upstream-Addr")
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.to_string())
                        {
                            // 启动 WS 双向转发任务
                            if let (Some(on_upgrade), Some((method, uri, headers))) =
                                (on_upgrade, ws_req_info)
                            {
                                let ws_shutdown = ws_shutdown.clone();
                                tokio::spawn(async move {
                                    match on_upgrade.await {
                                        Ok(upgraded) => {
                                            let io = TokioIo::new(upgraded);
                                            let mut upgrade_req = Request::builder()
                                                .method(method)
                                                .uri(uri)
                                                .body(Bytes::new())
                                                .unwrap();
                                            *upgrade_req.headers_mut() = headers;
                                            let forward = conrogate_protocol::upgrade::forward_websocket(
                                                &upstream_addr,
                                                io,
                                                upgrade_req,
                                                ws_connect_timeout,
                                                ws_idle_timeout,
                                                upgrade_buffer_size,
                                            );
                                            tokio::select! {
                                                result = forward => {
                                                    if let Err(e) = result {
                                                        tracing::warn!(error = %e, "websocket forwarding error");
                                                    }
                                                }
                                                _ = ws_shutdown.notified() => {
                                                    tracing::debug!("websocket tunnel closed by shutdown");
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(error = %e, "websocket upgrade failed");
                                        }
                                    }
                                });
                            }
                            // 清除扩展头（不透传给客户端）
                            let mut clean_resp = resp;
                            clean_resp.headers_mut().remove("X-WS-Upstream-Addr");
                            clean_resp.headers_mut().remove("X-WS-Trace-Id");
                            let (parts, _) = clean_resp.into_parts();
                            return Ok(Response::from_parts(parts, boxed_body(Bytes::new())));
                        }
                    }
                    let (parts, resp_body) = resp.into_parts();
                    return Ok(Response::from_parts(parts, resp_body));
                }
            }

            // 缓冲模式：路由未命中或需 requires_body 插件 → collect body
            // 慢读保护：客户端读取请求体超时则返回 408
            let body_bytes = match tokio::time::timeout(read_timeout, body.collect()).await {
                Ok(Ok(collected)) => collected.to_bytes(),
                Ok(Err(e)) => {
                    return Ok(error_response(
                        http::StatusCode::BAD_REQUEST,
                        10008,
                        &format!("request body read error: {e}"),
                    ));
                }
                Err(_) => {
                    return Ok(error_response(
                        http::StatusCode::REQUEST_TIMEOUT,
                        10009,
                        "request body read timeout",
                    ));
                }
            };

            // 请求体大小限制
            if body_bytes.len() > max_body_bytes {
                return Ok(error_response(
                    http::StatusCode::PAYLOAD_TOO_LARGE,
                    10007,
                    "request body too large",
                ));
            }

            let req = Request::from_parts(parts, body_bytes);
            let resp = match handler.handle_http(req, client_ip).await {
                Ok(resp) => resp,
                Err(ConrogateError::RateLimited) | Err(ConrogateError::Limited) => {
                    return Ok(error_response(
                        http::StatusCode::TOO_MANY_REQUESTS,
                        40008,
                        "rate limited",
                    ));
                }
                Err(ConrogateError::CircuitBreakerOpen) => {
                    return Ok(error_response(
                        http::StatusCode::SERVICE_UNAVAILABLE,
                        40007,
                        "circuit breaker open",
                    ));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "request handler error");
                    return Ok(error_response(
                        http::StatusCode::BAD_GATEWAY,
                        40006,
                        "gateway error",
                    ));
                }
            };

            // WebSocket 升级检测：101 响应 + X-WS-Upstream-Addr 头
            if resp.status() == http::StatusCode::SWITCHING_PROTOCOLS {
                if let Some(upstream_addr) = resp
                    .headers()
                    .get("X-WS-Upstream-Addr")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string())
                {
                    // 启动 WS 双向转发任务
                    if let (Some(on_upgrade), Some((method, uri, headers))) =
                        (on_upgrade, ws_req_info)
                    {
                        let ws_shutdown = ws_shutdown.clone();
                        tokio::spawn(async move {
                            match on_upgrade.await {
                                Ok(upgraded) => {
                                    let io = TokioIo::new(upgraded);
                                    let mut upgrade_req = Request::builder()
                                        .method(method)
                                        .uri(uri)
                                        .body(Bytes::new())
                                        .unwrap();
                                    *upgrade_req.headers_mut() = headers;
                                    let forward = conrogate_protocol::upgrade::forward_websocket(
                                        &upstream_addr,
                                        io,
                                        upgrade_req,
                                        ws_connect_timeout,
                                        ws_idle_timeout,
                                        upgrade_buffer_size,
                                    );
                                    tokio::select! {
                                        result = forward => {
                                            if let Err(e) = result {
                                                tracing::warn!(error = %e, "websocket forwarding error");
                                            }
                                        }
                                        _ = ws_shutdown.notified() => {
                                            tracing::debug!("websocket tunnel closed by shutdown");
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "websocket upgrade failed");
                                }
                            }
                        });
                    }
                    // 清除扩展头（不透传给客户端）
                    let mut clean_resp = resp;
                    clean_resp.headers_mut().remove("X-WS-Upstream-Addr");
                    clean_resp.headers_mut().remove("X-WS-Trace-Id");
                    return Ok(Response::from_parts(
                        clean_resp.into_parts().0,
                        boxed_body(Bytes::new()),
                    ));
                }
            }

            // 转换为 hyper 兼容响应（缓冲模式 body → 统一 ReqBody）
            let (parts, body) = resp.into_parts();
            Ok(Response::from_parts(parts, boxed_body(body)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::Full;
    use hyper::service::Service;

    /// 探针服务：任意请求返回固定响应体
    #[derive(Clone)]
    struct H2ProbeService;

    impl Service<Request<Incoming>> for H2ProbeService {
        type Response = Response<ReqBody>;
        type Error = ConrogateError;
        type Future = std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
        >;

        fn call(&self, _req: Request<Incoming>) -> Self::Future {
            Box::pin(async move {
                Ok(Response::builder()
                    .status(http::StatusCode::OK)
                    .body(boxed_body(Bytes::from_static(b"probe-ok")))
                    .unwrap())
            })
        }
    }

    /// 启动一个使用生产 `serve_cleartext_connection`（auto builder）的测试监听
    async fn spawn_auto_server() -> SocketAddr {
        spawn_auto_server_with_read_timeout(std::time::Duration::from_secs(15)).await
    }

    /// 同 spawn_auto_server，但可指定客户端读取超时
    async fn spawn_auto_server_with_read_timeout(read_timeout: std::time::Duration) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let io = TokioIo::new(stream);
            let _ = serve_cleartext_connection(io, H2ProbeService, 65536, read_timeout).await;
        });
        addr
    }

    /// 入站 HTTP/2：h2c prior-knowledge（PRI * HTTP/2.0 preface）应协商为 HTTP/2
    #[tokio::test]
    async fn inbound_http2_h2c_prior_knowledge() {
        let addr = spawn_auto_server().await;
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http2::Builder::new(TokioExecutor::new())
            .handshake(io)
            .await
            .expect("h2 client handshake");
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let resp = sender
            .send_request(
                Request::builder()
                    .uri("http://example.com/")
                    .body(Full::new(Bytes::from_static(b"")))
                    .unwrap(),
            )
            .await
            .expect("h2 request");
        assert_eq!(resp.version(), http::Version::HTTP_2);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"probe-ok");
    }

    /// 入站 HTTP/1.1：明文首包不是 h2 preface 时回退 HTTP/1.1
    #[tokio::test]
    async fn inbound_http1_fallback() {
        let addr = spawn_auto_server().await;
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let resp = sender
            .send_request(
                Request::builder()
                    .uri("/")
                    .body(Full::new(Bytes::from_static(b"")))
                    .unwrap(),
            )
            .await
            .expect("http1 request");
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"probe-ok");
    }

    /// 慢读保护：客户端只发部分请求头后停顿，超过 read_timeout 连接应被服务端关闭
    #[tokio::test]
    async fn http1_slow_header_read_timeout_closes_connection() {
        let addr = spawn_auto_server_with_read_timeout(std::time::Duration::from_millis(200)).await;
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // 只发送部分请求头（未以空行结束），然后保持静默
        use tokio::io::AsyncWriteExt;
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: test\r\n")
            .await
            .unwrap();

        // 等待超过 read_timeout
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        // 服务端应已关闭连接：读取返回 0 字节（EOF）
        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 16];
        let n = stream.read(&mut buf).await.expect("read should succeed");
        assert_eq!(n, 0, "connection should be closed by header read timeout");
    }
}
