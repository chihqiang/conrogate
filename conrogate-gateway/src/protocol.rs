//! 协议适配层：HTTP / WebSocket / TCP 隧道 handler。

use conrogate_contract::gateway::ServiceContext;
use conrogate_contract::plugin::{HttpContext, PluginContext, PluginOutcome, PluginResponse};
use conrogate_contract::protocol::{ProtocolId, RouteMatchInfo};
use conrogate_contract::ConrogateError;
use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use std::sync::Arc;

/// HTTP 协议处理器
pub struct HttpProtocolHandler {
    svc: Arc<ServiceContext>,
}

impl HttpProtocolHandler {
    pub fn new(svc: Arc<ServiceContext>) -> Self {
        Self { svc }
    }

    /// 处理 HTTP 请求
    pub async fn handle(
        &self,
        req: Request<Bytes>,
        client_ip: String,
    ) -> Result<Response<Bytes>, ConrogateError> {
        // 1. 构造路由匹配信息
        let (parts, body) = req.into_parts();
        let method = parts.method;
        let uri = parts.uri;
        let headers = parts.headers;
        let match_info = RouteMatchInfo::from_http_request(&method, &uri, &headers);
        let request_id = uuid::Uuid::new_v4().to_string();
        let trace_id = headers
            .get("X-Trace-Id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(&request_id)
            .to_string();

        // 2. 路由匹配
        let route = self
            .svc
            .routes
            .lookup_route(ProtocolId::Http, &match_info)
            .await?
            .ok_or_else(|| ConrogateError::RouteNotFound(match_info.path.clone()))?;

        // 3. 构造插件上下文
        let mut plugin_ctx = PluginContext {
            request_id: request_id.clone(),
            trace_id: trace_id.clone(),
            route_id: route.id,
            client_ip,
            protocol: ProtocolId::Http,
            http: Some(HttpContext {
                method,
                path: match_info.path.clone(),
                query: match_info
                    .query_params
                    .iter()
                    .cloned()
                    .collect(),
                headers,
                body: Some(body),
            }),
            tunnel: None,
            services: conrogate_contract::plugin::PluginServices {
                metrics: Arc::new(NoopMetrics),
                logger: Arc::new(NoopLogger),
            },
        };

        // 4. 执行插件 before_request
        let plugins = route.plugin_chain.clone();
        let plugin_outcome = self
            .svc
            .plugins
            .execute_before_request(&mut plugin_ctx, &[])
            .await?;

        // 5. 插件可能终止请求
        if let PluginOutcome::Terminate(code, body) = plugin_outcome {
            return Ok(Response::builder()
                .status(code)
                .body(Bytes::from(body.to_string().into_bytes()))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Bytes::new())
                        .unwrap()
                }));
        }

        // 6. 流量治理检查
        self.svc
            .traffic
            .check_rate_limit(route.id, &plugin_ctx.client_ip)
            .await?;

        // 7. 选择上游节点
        let node = self.svc.balancer.select_upstream(&route).await?;

        // 8. 熔断检查
        self.svc
            .traffic
            .check_circuit_breaker(route.id, node.upstream_id)
            .await?;

        // 9. 构造上游请求（简化版：实际应使用 proxy 模块转发）
        // TODO: 调用 proxy::forward_http 完成实际转发
        let success = true;

        // 10. 记录结果
        self.svc
            .traffic
            .record_result(route.id, node.upstream_id, success)
            .await;

        // 11. 构造响应
        let resp = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Bytes::from(
                serde_json::json!({
                    "status": "ok",
                    "upstream": node.address,
                    "route_id": route.id,
                })
                .to_string(),
            ))
            .unwrap();

        // 12. 插件 after_response
        let mut plugin_resp = PluginResponse {
            status: 200,
            headers: http::HeaderMap::new(),
            body: resp.body().clone(),
        };

        self.svc
            .plugins
            .execute_after_response(&mut plugin_ctx, &mut plugin_resp, &[])
            .await?;

        Ok(resp)
    }
}

/// TCP 隧道协议处理器
pub struct TcpTunnelProtocolHandler {
    svc: Arc<ServiceContext>,
}

impl TcpTunnelProtocolHandler {
    pub fn new(svc: Arc<ServiceContext>) -> Self {
        Self { svc }
    }

    /// 处理 TCP 隧道连接
    pub async fn handle(
        &self,
        listen_addr: String,
        sni: Option<String>,
        client_ip: String,
    ) -> Result<(), ConrogateError> {
        let match_info = RouteMatchInfo::from_tunnel(&listen_addr, sni.as_deref());

        let route = self
            .svc
            .routes
            .lookup_route(ProtocolId::TcpTunnel, &match_info)
            .await?
            .ok_or_else(|| ConrogateError::RouteNotFound(listen_addr.clone()))?;

        // 选择上游
        let node = self.svc.balancer.select_upstream(&route).await?;

        // 检查熔断
        self.svc
            .traffic
            .check_circuit_breaker(route.id, node.upstream_id)
            .await?;

        tracing::info!(
            route_id = route.id,
            upstream = %node.address,
            "tcp tunnel established"
        );

        // 实际转发由调用方执行（需要 inbound stream）
        // TODO: 调用 proxy::forward_tcp

        Ok(())
    }
}

// ── 空实现辅助类型 ──

struct NoopMetrics;

#[async_trait::async_trait]
impl conrogate_contract::plugin::PluginMetrics for NoopMetrics {
    async fn increment(&self, _name: &str) {}
    async fn gauge(&self, _name: &str, _value: f64) {}
}

struct NoopLogger;

#[async_trait::async_trait]
impl conrogate_contract::plugin::PluginLogger for NoopLogger {
    async fn log(&self, _level: &str, _message: &str) {}
}
