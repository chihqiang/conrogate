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

/// 网关服务
pub struct GatewayServer {
    config: ConfigReloader,
    route_matcher: Arc<RouteMatcher>,
    upstream_selector: Arc<UpstreamSelectorImpl>,
    http_handler: Arc<HttpProtocolHandler>,
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

        // 加载初始路由 + 上游
        let route_repo = conrogate_storage::repository::route_repo::RouteRepoImpl::new((*read_db).clone());
        let upstream_repo = conrogate_storage::repository::upstream_repo::UpstreamRepoImpl::new((*read_db).clone());

        let routes = conrogate_contract::storage::ReadOnlyRouteRepo::list_enabled(&route_repo).await
            .unwrap_or_default();
        let upstreams = conrogate_contract::storage::ReadOnlyUpstreamRepo::list_all(&upstream_repo).await
            .unwrap_or_default();

        server.route_matcher.load(routes);
        server.upstream_selector.load_upstreams(upstreams);

        // 启动配置热加载后台任务
        let matcher = server.route_matcher.clone();
        let selector = server.upstream_selector.clone();
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
                if !r.is_empty() || !u.is_empty() {
                    matcher.load(r);
                    selector.load_upstreams(u);
                    tracing::debug!("config hot-reloaded from DB");
                }
            }
        });

        server
    }

    /// 启动网关服务
    pub async fn run(&self) -> Result<(), ConrogateError> {
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

        tracing::info!(addr = %addr, max_connections = self.max_connections, "gateway server started");

        loop {
            let (stream, remote_addr) = listener
                .accept()
                .await
                .map_err(|e| ConrogateError::Init(format!("tcp accept: {e}")))?;

            // 健康探针：GET /healthz 直接返回 200，不走路由
            let client_ip = remote_addr.ip().to_string();
            let http_handler = self.http_handler.clone();
            let semaphore = conn_semaphore.clone();

            tokio::spawn(async move {
                // 获取并发许可（带超时，避免堆积）
                let _permit = match semaphore.acquire().await {
                    Ok(p) => p,
                    Err(_) => {
                        tracing::warn!("connection semaphore closed");
                        return;
                    }
                };

                let io = TokioIo::new(stream);
                let svc = HyperServiceBridge {
                    handler: http_handler,
                    client_ip,
                    max_body_bytes,
                };

                // HTTP/1 服务（带空闲超时）
                let h1 = http1::Builder::new();
                let serve = h1.serve_connection(io, svc);

                // 包裹空闲超时
                let result = tokio::time::timeout(idle_timeout, serve).await;
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
    }

    /// 热加载路由
    pub fn reload_routes(&self, routes: Vec<conrogate_contract::dto::RouteDto>) {
        self.route_matcher.load(routes);
        tracing::info!("routes reloaded");
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
    client_ip: String,
    max_body_bytes: usize,
}

impl hyper::service::Service<Request<Incoming>> for HyperServiceBridge {
    type Response = Response<Full<Bytes>>;
    type Error = ConrogateError;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let handler = self.handler.clone();
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

            // 收集请求体
            let (parts, body) = req.into_parts();
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
