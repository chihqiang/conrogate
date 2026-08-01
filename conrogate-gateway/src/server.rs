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

        tracing::info!(addr = %addr, "gateway server started");

        loop {
            let (stream, remote_addr) = listener
                .accept()
                .await
                .map_err(|e| ConrogateError::Init(format!("tcp accept: {e}")))?;

            let client_ip = remote_addr.ip().to_string();
            let http_handler = self.http_handler.clone();

            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                let svc = HyperServiceBridge {
                    handler: http_handler,
                    client_ip,
                };

                // HTTP/1 + HTTP/2 自动协商
                let mut h1 = http1::Builder::new();
                let mut h2 = hyper::server::conn::http2::Builder::new(hyper_util::rt::TokioExecutor::new());

                // 尝试 HTTP/2 直连（h2c prior knowledge）或 HTTP/1 升级
                let serve = async {
                    // 先尝试 HTTP/1（含 upgrade），如果检测到 h2c 则切换
                    if let Err(e) = h1.serve_connection(io, svc).await {
                        tracing::debug!(error = %e, "http1 connection ended");
                    }
                };
                serve.await;
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
}

impl hyper::service::Service<Request<Incoming>> for HyperServiceBridge {
    type Response = Response<Full<Bytes>>;
    type Error = ConrogateError;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let handler = self.handler.clone();
        let client_ip = self.client_ip.clone();

        Box::pin(async move {
            // 收集请求体
            let (parts, body) = req.into_parts();
            let body_bytes = body
                .collect()
                .await
                .map_err(|e| ConrogateError::UpstreamBadResponse(e.to_string()))?
                .to_bytes();

            let req = Request::from_parts(parts, body_bytes);
            let resp = handler.handle(req, client_ip).await?;

            // 转换为 hyper 兼容响应
            let (parts, body) = resp.into_parts();
            Ok(Response::from_parts(parts, Full::new(body)))
        })
    }
}
