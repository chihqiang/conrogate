//! 网关服务入口：启动 HTTP/TCP 监听 + 组装 ServiceContext。

use crate::filter::ConfigReloader;
use crate::protocol::{HttpProtocolHandler, TcpTunnelProtocolHandler};
use crate::route::RouteMatcher;
use crate::pool::UpstreamSelectorImpl;
use crate::telemetry::TelemetryReportImpl;
use bytes::Bytes;
use conrogate_balancer::registry::create_default_registry;
use conrogate_contract::config::Config;
use conrogate_contract::gateway::ServiceContext;
use conrogate_contract::ConrogateError;
use conrogate_plugin::pipeline::PluginPipelineImpl;
use conrogate_plugin::registry::PluginRegistryImpl;
use conrogate_traffic::breaker::{BreakerConfig, BreakerFactoryImpl};
use conrogate_traffic::limiter::TokenBucketLimiter;
use http::{Request, Response};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use conrogate_contract::protocol::{ProtocolId, RouteMatchInfo};

/// 网关服务
pub struct GatewayServer {
    config: ConfigReloader,
    route_matcher: Arc<RouteMatcher>,
    upstream_selector: Arc<UpstreamSelectorImpl>,
    http_handler: Arc<HttpProtocolHandler>,
    #[allow(dead_code)]
    tcp_handler: Arc<TcpTunnelProtocolHandler>,
    plugin_registry: Arc<PluginRegistryImpl>,
    max_connections: usize,
    max_body_bytes: usize,
    idle_timeout: std::time::Duration,
}

impl GatewayServer {
    /// 从配置构建网关（async：需注册插件）
    pub async fn from_config(config: Config) -> Self {
        let config_reloader = ConfigReloader::new(config.clone());

        // 路由匹配引擎
        let route_matcher = Arc::new(RouteMatcher::new());

        // 上游选择器
        let registry = create_default_registry();
        let upstream_selector = Arc::new(UpstreamSelectorImpl::new(registry));

        // 流量治理
        let limiter = Arc::new(TokenBucketLimiter::new());
        let breaker_factory = Arc::new(BreakerFactoryImpl::new(BreakerConfig::default()));
        let traffic = Arc::new(crate::filter::TrafficControlAdapter {
            limiter,
            breaker_factory,
        });

        // 遥测
        let (metric_tx, _metric_rx) = mpsc::channel(100_000);
        let (event_tx, _event_rx) = mpsc::channel(100_000);
        let telemetry = Arc::new(TelemetryReportImpl::new(metric_tx, event_tx));

        // 插件执行器
        let plugin_executor = Arc::new(PluginPipelineImpl::new());

        // 插件注册表 + 注册内置插件
        let plugin_registry = Arc::new(PluginRegistryImpl::new());
        plugin_registry
            .register(Arc::new(conrogate_plugin_log::LogPlugin::new()))
            .await;
        plugin_registry
            .register(Arc::new(conrogate_plugin_cors::CorsPlugin::new()))
            .await;
        plugin_registry
            .register(Arc::new(conrogate_plugin_auth::AuthPlugin::new()))
            .await;

        let svc = Arc::new(ServiceContext {
            routes: route_matcher.clone(),
            balancer: upstream_selector.clone(),
            traffic,
            telemetry,
            plugins: plugin_executor,
        });

        let timeout = std::time::Duration::from_millis(
            config.gate.timeouts.total.as_millis() as u64,
        );

        let http_handler = Arc::new(HttpProtocolHandler::with_registry(
            svc.clone(),
            plugin_registry.clone(),
            timeout,
        ));
        let tcp_handler = Arc::new(TcpTunnelProtocolHandler::new(svc));

        Self {
            config: config_reloader,
            route_matcher,
            upstream_selector,
            http_handler,
            tcp_handler,
            plugin_registry,
            max_connections: config.gate.connection.max_connections,
            max_body_bytes: config.gate.connection.max_body_bytes,
            idle_timeout: config.gate.connection.idle_timeout,
        }
    }

    /// 从已有组件构建网关（bootstrap 装配路径）
    pub fn from_components(
        config: Config,
        svc: Arc<ServiceContext>,
        plugin_registry: Arc<PluginRegistryImpl>,
    ) -> Self {
        let config_reloader = ConfigReloader::new(config.clone());
        let route_matcher = Arc::new(RouteMatcher::new());
        let upstream_selector = Arc::new(UpstreamSelectorImpl::new(create_default_registry()));
        let timeout = std::time::Duration::from_millis(
            config.gate.timeouts.total.as_millis() as u64,
        );

        let http_handler = Arc::new(HttpProtocolHandler::with_registry(
            svc.clone(),
            plugin_registry.clone(),
            timeout,
        ));
        let tcp_handler = Arc::new(TcpTunnelProtocolHandler::new(svc));

        Self {
            config: config_reloader,
            route_matcher,
            upstream_selector,
            http_handler,
            tcp_handler,
            plugin_registry,
            max_connections: config.gate.connection.max_connections,
            max_body_bytes: config.gate.connection.max_body_bytes,
            idle_timeout: config.gate.connection.idle_timeout,
        }
    }

    /// 从配置 + DB 连接构建网关（含配置热加载）
    pub async fn from_config_with_db(
        config: Config,
        read_db: Arc<conrogate_storage::pool::DbConn>,
    ) -> Self {
        let server = Self::from_config(config).await;

        // 加载初始路由 + 上游 + 插件绑定
        let route_repo = conrogate_storage::repository::route_repo::RouteRepoImpl::new((*read_db).clone());
        let upstream_repo = conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new((*read_db).clone());
        let binding_repo = conrogate_storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new((*read_db).clone());

        let routes = conrogate_contract::storage::ReadOnlyRouteRepo::list_enabled(&route_repo).await
            .unwrap_or_default();
        let upstreams = conrogate_contract::storage::ReadOnlyUpstreamRepo::list_all(&upstream_repo).await
            .unwrap_or_default();
        let mut all_bindings = Vec::new();
        for route in &routes {
            let rb = conrogate_contract::storage::ReadOnlyPluginBindingRepo::list_by_route(
                &binding_repo, route.id,
            ).await.unwrap_or_default();
            all_bindings.extend(rb);
        }

        let body_required = server.plugin_registry.body_required_plugin_names();
        server.route_matcher.load_with_bindings(routes, all_bindings, &body_required);
        server.upstream_selector.load_upstreams(upstreams);

        // 启动配置热加载后台任务
        let matcher = server.route_matcher.clone();
        let selector = server.upstream_selector.clone();
        let registry = server.plugin_registry.clone();
        let db = read_db.clone();
        tokio::spawn(async move {
            let poll_interval = std::time::Duration::from_secs(10);
            loop {
                tokio::time::sleep(poll_interval).await;
                let r = conrogate_contract::storage::ReadOnlyRouteRepo::list_enabled(
                    &conrogate_storage::repository::route_repo::RouteRepoImpl::new((*db).clone()),
                ).await.unwrap_or_default();
                let u = conrogate_contract::storage::ReadOnlyUpstreamRepo::list_all(
                    &conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new((*db).clone()),
                ).await.unwrap_or_default();
                // 加载插件绑定
                let mut bindings = Vec::new();
                for route in &r {
                    let rb = conrogate_contract::storage::ReadOnlyPluginBindingRepo::list_by_route(
                        &conrogate_storage::repository::plugin_binding_repo::PluginBindingRepoImpl::new((*db).clone()),
                        route.id,
                    ).await.unwrap_or_default();
                    bindings.extend(rb);
                }
                if !r.is_empty() || !u.is_empty() {
                    let body_req = registry.body_required_plugin_names();
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
    pub async fn run_with_shutdown<F>(
        &self,
        shutdown: F,
    ) -> Result<(), ConrogateError>
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
        let idle_timeout = self.idle_timeout;
        let long_conn_drain = config.gate.shutdown.long_conn_drain;

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
            tracing::info!("TLS passthrough mode enabled (raw TCP forwarding)");
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
                    let http_handler = self.http_handler.clone();
                    let route_matcher = self.route_matcher.clone();
                    let semaphore = conn_semaphore.clone();
                    let tls_acc = tls_acceptor.clone();

                    tokio::spawn(async move {
                        // 获取并发许可
                        let _permit = match semaphore.acquire().await {
                            Ok(p) => p,
                            Err(_) => {
                                tracing::warn!("connection semaphore closed");
                                return;
                            }
                        };

                        let svc = HyperServiceBridge {
                            handler: http_handler,
                            route_matcher,
                            client_ip,
                            max_body_bytes,
                        };

                        // TLS 握手（如果启用 terminate 模式）
                        let h1 = http1::Builder::new();
                        let result = if let Some(acc) = tls_acc {
                            match acc.accept(stream).await {
                                Ok(tls_stream) => {
                                    let io = TokioIo::new(tls_stream);
                                    tokio::time::timeout(
                                        idle_timeout,
                                        h1.serve_connection(io, svc),
                                    ).await
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "TLS handshake failed");
                                    return;
                                }
                            }
                        } else {
                            let io = TokioIo::new(stream);
                            tokio::time::timeout(
                                idle_timeout,
                                h1.serve_connection(io, svc),
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
        tracing::info!(drain_ms = long_conn_drain.as_millis(), "graceful shutdown: draining in-flight connections");
        tokio::time::sleep(long_conn_drain).await;

        // 宽限期结束，强制释放所有并发许可（触发连接清理）
        tracing::info!("graceful drain period expired, connections will be force-closed");
        // 语义上的信号：permit drop 后 Semaphore 容量恢复，
        // 但已在执行中的连接会在 idle_timeout 后自然超时

        Ok(())
    }

    /// 启动网关服务（阻塞，无优雅停机）
    pub async fn run(&self) -> Result<(), ConrogateError> {
        let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
        self.run_with_shutdown(async move {
            let _ = rx.await;
        }).await
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
        self.route_matcher.load_with_bindings(routes, bindings, &body_required);
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
}

/// hyper Service 桥接器
#[derive(Clone)]
struct HyperServiceBridge {
    handler: Arc<HttpProtocolHandler>,
    route_matcher: Arc<RouteMatcher>,
    client_ip: String,
    max_body_bytes: usize,
}

impl hyper::service::Service<Request<Incoming>> for HyperServiceBridge {
    type Response = Response<Full<Bytes>>;
    type Error = ConrogateError;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let handler = self.handler.clone();
        let route_matcher = self.route_matcher.clone();
        let client_ip = self.client_ip.clone();
        let max_body_bytes = self.max_body_bytes;

        Box::pin(async move {
            // 健康探针：GET /healthz → 200
            if req.method() == http::Method::GET && req.uri().path() == "/healthz" {
                return Ok(Response::builder()
                    .status(http::StatusCode::OK)
                    .body(Full::new(Bytes::from_static(b"ok")))
                    .unwrap());
            }

            // 就绪探针：GET /readyz → 200（路由缓存非空）/ 503（路由为空）
            if req.method() == http::Method::GET && req.uri().path() == "/readyz" {
                if route_matcher.is_empty() {
                    return Ok(Response::builder()
                        .status(http::StatusCode::SERVICE_UNAVAILABLE)
                        .body(Full::new(Bytes::from_static(b"not ready: no routes loaded")))
                        .unwrap());
                }
                return Ok(Response::builder()
                    .status(http::StatusCode::OK)
                    .body(Full::new(Bytes::from_static(b"ready")))
                    .unwrap());
            }

            // 拆分请求：先匹配路由，判定是否需要缓冲 body
            let (parts, body) = req.into_parts();
            let match_info = RouteMatchInfo::from_http_request(
                &parts.method,
                &parts.uri,
                &parts.headers,
            );

            // 尝试路由匹配
            let matched_route = route_matcher.match_route(ProtocolId::Http, &match_info);

            // 流式模式：路由命中且无 requires_body 插件 → 不 collect body，直接透传
            if let Some(ref route) = matched_route {
                if !route.requires_body {
                    let resp = match handler.handle_stream(parts, body, route.clone(), client_ip).await {
                        Ok(resp) => resp,
                        Err(ConrogateError::RateLimited) | Err(ConrogateError::Limited) => {
                            return Ok(Response::builder()
                                .status(http::StatusCode::TOO_MANY_REQUESTS)
                                .header("Retry-After", "1")
                                .body(Full::new(Bytes::from("rate limited")))
                                .unwrap());
                        }
                        Err(ConrogateError::CircuitBreakerOpen) => {
                            return Ok(Response::builder()
                                .status(http::StatusCode::SERVICE_UNAVAILABLE)
                                .body(Full::new(Bytes::from("circuit breaker open")))
                                .unwrap());
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "stream handler error");
                            return Ok(Response::builder()
                                .status(http::StatusCode::BAD_GATEWAY)
                                .body(Full::new(Bytes::from("gateway error")))
                                .unwrap());
                        }
                    };
                    let (parts, resp_body) = resp.into_parts();
                    return Ok(Response::from_parts(parts, Full::new(resp_body)));
                }
            }

            // 缓冲模式：路由未命中或需 requires_body 插件 → collect body
            let body_bytes = body
                .collect()
                .await
                .map_err(|e| ConrogateError::UpstreamBadResponse(e.to_string()))?
                .to_bytes();

            // 请求体大小限制
            if body_bytes.len() > max_body_bytes {
                return Ok(Response::builder()
                    .status(http::StatusCode::PAYLOAD_TOO_LARGE)
                    .body(Full::new(Bytes::from("request body too large")))
                    .unwrap());
            }

            let req = Request::from_parts(parts, body_bytes);
            let resp = match handler.handle(req, client_ip).await {
                Ok(resp) => resp,
                Err(ConrogateError::RateLimited) | Err(ConrogateError::Limited) => {
                    // 限流响应：429 + Retry-After 头
                    return Ok(Response::builder()
                        .status(http::StatusCode::TOO_MANY_REQUESTS)
                        .header("Retry-After", "1")
                        .body(Full::new(Bytes::from("rate limited")))
                        .unwrap());
                }
                Err(ConrogateError::CircuitBreakerOpen) => {
                    return Ok(Response::builder()
                        .status(http::StatusCode::SERVICE_UNAVAILABLE)
                        .body(Full::new(Bytes::from("circuit breaker open")))
                        .unwrap());
                }
                Err(e) => {
                    tracing::warn!(error = %e, "request handler error");
                    return Ok(Response::builder()
                        .status(http::StatusCode::BAD_GATEWAY)
                        .body(Full::new(Bytes::from("gateway error")))
                        .unwrap());
                }
            };

            // 转换为 hyper 兼容响应
            let (parts, body) = resp.into_parts();
            Ok(Response::from_parts(parts, Full::new(body)))
        })
    }
}
