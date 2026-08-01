//! 网关服务入口：启动 HTTP/TCP 监听 + 组装 ServiceContext。

use crate::filter::ConfigReloader;
use crate::protocol::{HttpProtocolHandler, TcpTunnelProtocolHandler};
use crate::route::RouteMatcher;
use crate::pool::UpstreamSelectorImpl;
use crate::telemetry::{MetricAggregator, TelemetryReportImpl};
use bytes::Bytes;
use conrogate_balancer::registry::create_default_registry;
use conrogate_contract::config::Config;
use conrogate_contract::gateway::ServiceContext;
use conrogate_contract::ConrogateError;
use conrogate_plugin::pipeline::PluginPipelineImpl;
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
}

impl GatewayServer {
    /// 从配置构建网关
    pub fn from_config(config: Config) -> Self {
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
        let (metric_tx, metric_rx) = mpsc::channel(100_000);
        let (event_tx, event_tx_rx) = mpsc::channel(100_000);
        let telemetry = Arc::new(TelemetryReportImpl::new(metric_tx, event_tx));

        // 插件执行器
        let plugin_executor = Arc::new(PluginPipelineImpl::new());

        let svc = Arc::new(ServiceContext {
            routes: route_matcher.clone(),
            balancer: upstream_selector.clone(),
            traffic,
            telemetry,
            plugins: plugin_executor,
        });

        let http_handler = Arc::new(HttpProtocolHandler::new(svc.clone()));
        let tcp_handler = Arc::new(TcpTunnelProtocolHandler::new(svc));

        Self {
            config: config_reloader,
            route_matcher,
            upstream_selector,
            http_handler,
            tcp_handler,
        }
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

                if let Err(e) = http1::Builder::new()
                    .serve_connection(io, svc)
                    .await
                {
                    tracing::warn!(error = %e, "http connection error");
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
