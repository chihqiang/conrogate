//! HTTP 协议处理器：完整转发链路（缓冲 / 流式两种模式）。

use crate::handler::{NoopLogger, NoopMetrics, ProtocolHandler};
use crate::proxy::{ReqBody, body_from_bytes, body_from_incoming};
use conrogate_contract::gateway::ServiceContext;
use conrogate_contract::plugin::{HttpContext, PluginContext, PluginOutcome, PluginResponse};
use conrogate_contract::protocol::{ProtocolId, RouteMatchInfo};
use conrogate_contract::ConrogateError;
use bytes::Bytes;
use http::{Request, Response, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use std::sync::Arc;
use std::time::Duration;

/// 敏感 Header 黑名单（不透传到上游）
const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "cookie",
    "proxy-authorization",
    "x-api-key",
    "x-forwarded-for",
    "x-forwarded-proto",
    "x-request-id",
    "x-trace-id",
];

/// HTTP 协议处理器
pub struct HttpProtocolHandler {
    svc: Arc<ServiceContext>,
    /// 插件注册表（解析路由绑定 → 插件实例）
    plugin_registry: Option<Arc<conrogate_plugin::registry::PluginRegistryImpl>>,
    /// hyper 客户端（连接池复用，统一使用 BoxBody 体类型）
    client: Client<HttpConnector, ReqBody>,
    /// 转发超时
    timeout: Duration,
    /// 可信代理 CIDR 列表（XFF 信任链）
    trusted_proxies: Vec<String>,
    /// 限流配置
    rate_limit_qps: u32,
    /// 最大重试次数
    max_retries: u32,
}

impl HttpProtocolHandler {
    pub fn new(svc: Arc<ServiceContext>) -> Self {
        let client = Client::builder(hyper_util::rt::TokioExecutor::new())
            .build(HttpConnector::new());
        Self {
            svc,
            plugin_registry: None,
            client,
            timeout: Duration::from_secs(30),
            trusted_proxies: Vec::new(),
            rate_limit_qps: 100,
            max_retries: 3,
        }
    }

    /// 使用指定超时创建
    pub fn with_timeout(svc: Arc<ServiceContext>, timeout: Duration) -> Self {
        let client = Client::builder(hyper_util::rt::TokioExecutor::new())
            .build(HttpConnector::new());
        Self {
            svc,
            plugin_registry: None,
            client,
            timeout,
            trusted_proxies: Vec::new(),
            rate_limit_qps: 100,
            max_retries: 3,
        }
    }

    /// 使用插件注册表 + 超时创建
    pub fn with_registry(
        svc: Arc<ServiceContext>,
        plugin_registry: Arc<conrogate_plugin::registry::PluginRegistryImpl>,
        timeout: Duration,
    ) -> Self {
        let client = Client::builder(hyper_util::rt::TokioExecutor::new())
            .build(HttpConnector::new());
        Self {
            svc,
            plugin_registry: Some(plugin_registry),
            client,
            timeout,
            trusted_proxies: Vec::new(),
            rate_limit_qps: 100,
            max_retries: 3,
        }
    }

    /// 设置可信代理 CIDR 列表
    pub fn with_trusted_proxies(mut self, proxies: Vec<String>) -> Self {
        self.trusted_proxies = proxies;
        self
    }

    /// 设置限流 QPS
    pub fn with_rate_limit_qps(mut self, qps: u32) -> Self {
        self.rate_limit_qps = qps;
        self
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// XFF 信任链解析：从 X-Forwarded-For 链中提取真实客户端 IP
    fn resolve_real_ip(&self, socket_ip: &str, headers: &http::HeaderMap) -> String {
        if self.trusted_proxies.is_empty() {
            return socket_ip.to_string();
        }

        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            let chain: Vec<&str> = xff.split(',').map(|s| s.trim()).collect();
            for ip in &chain {
                if !self.is_trusted_proxy(ip) {
                    return ip.to_string();
                }
            }
        }
        socket_ip.to_string()
    }

    /// 检查 IP 是否为可信代理
    fn is_trusted_proxy(&self, ip: &str) -> bool {
        for proxy in &self.trusted_proxies {
            if let Ok(cidr) = proxy.parse::<ipnet::IpNet>() {
                if let Ok(addr) = ip.parse::<std::net::IpAddr>() {
                    if cidr.contains(&addr) {
                        return true;
                    }
                }
            }
            if proxy == ip {
                return true;
            }
        }
        false
    }

    /// 解析路由绑定的插件链 → Arc<dyn Plugin> 列表
    fn resolve_plugins(
        &self,
        bindings: &[conrogate_contract::dto::PluginBindingDto],
    ) -> Vec<Arc<dyn conrogate_contract::plugin::Plugin>> {
        let mut plugins = Vec::new();
        if let Some(ref registry) = self.plugin_registry {
            for binding in bindings {
                if !binding.enabled {
                    continue;
                }
                if let Some(plugin) = registry.get(&binding.plugin_name) {
                    plugins.push(plugin);
                }
            }
        }
        plugins
    }

    /// 处理 HTTP 请求 — 完整转发链路（缓冲模式：body 已载入内存）
    async fn handle(
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
            .get("x-trace-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(&request_id)
            .to_string();

        // 1a. XFF 信任链：解析真实客户端 IP
        let real_ip = self.resolve_real_ip(&client_ip, &headers);

        // 记录请求开始时间（用于延迟统计）
        let start = std::time::Instant::now();

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
            client_ip: real_ip.clone(),
            protocol: ProtocolId::Http,
            http: Some(HttpContext {
                method: method.clone(),
                path: match_info.path.clone(),
                query: match_info
                    .query_params
                    .iter()
                    .cloned()
                    .collect(),
                headers: headers.clone(),
                body: Some(body.clone()),
            }),
            tunnel: None,
            services: conrogate_contract::plugin::PluginServices {
                metrics: Arc::new(NoopMetrics),
                logger: Arc::new(NoopLogger),
            },
        };

        // 4. 执行插件 before_request（解析路由绑定 → 插件实例）
        let plugins = self.resolve_plugins(&route.plugin_chain);
        let plugin_outcome = self
            .svc
            .plugins
            .execute_before_request(&mut plugin_ctx, &plugins)
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

        // 6. 流量治理检查（使用配置的 QPS）
        if let Err(e) = self.svc
            .traffic
            .check_rate_limit(route.id, &real_ip)
            .await
        {
            // 上报限流事件到遥测
            self.svc.telemetry.record_event(
                conrogate_contract::dto::EventRow {
                    ts: chrono::Utc::now(),
                    event_type: "rate_limited".into(),
                    route_id: Some(route.id),
                    upstream_id: None,
                    trace_id: Some(trace_id.clone()),
                    detail: serde_json::json!({
                        "client_ip": real_ip,
                        "reason": e.to_string(),
                    }),
                }
            ).await;
            return Err(e);
        }

        // 7. 选择上游节点（一致性哈希按真实 client_ip）
        let node = self.svc.balancer.select_upstream(&route, Some(&real_ip)).await?;

        // 8. 熔断检查
        self.svc
            .traffic
            .check_circuit_breaker(route.id, node.id)
            .await?;

        // 8a. WebSocket 升级检测（路由匹配 + 上游选择完成后）
        let upgrade_req = Request::builder()
            .method(method.clone())
            .uri(uri.clone())
            .version(parts.version)
            .body(body.clone())
            .unwrap();
        if crate::upgrade::is_upgrade_request(&upgrade_req) {
            let mut resp = crate::upgrade::build_upgrade_response(&upgrade_req);
            // 设置上游地址头，供 HyperServiceBridge 提取并执行 WS 转发
            if let Ok(v) = node.address.parse() {
                resp.headers_mut().insert("X-WS-Upstream-Addr", v);
            }
            if let Ok(v) = trace_id.parse() {
                resp.headers_mut().insert("X-WS-Trace-Id", v);
            }
            tracing::info!(
                trace_id = %trace_id,
                upstream = %node.address,
                "websocket upgrade request, returning 101 with upstream addr"
            );
            return Ok(resp);
        }

        // 9. 构造上游请求（处理 Header）
        let path_and_query = uri
            .path_and_query()
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| "/".to_string());
        let upstream_addr = format!("http://{}", node.address);
        let upstream_uri: http::Uri = format!("{}{}", upstream_addr, path_and_query)
            .parse()
            .map_err(|e| ConrogateError::UpstreamConnectFailed(format!("uri parse: {e}")))?;

        let method_clone = method.clone();
        let upstream_uri_clone = upstream_uri.clone();
        let mut upstream_req = Request::builder()
            .method(method)
            .uri(upstream_uri)
            .body(body_from_bytes(body))
            .map_err(|e| ConrogateError::UpstreamConnectFailed(format!("request build: {e}")))?;

        // 9a. Header 处理：过滤敏感头 + 注入网关头
        let mut out_headers = http::HeaderMap::new();
        for (name, value) in headers.iter() {
            let name_lower = name.as_str().to_lowercase();
            if !SENSITIVE_HEADERS.contains(&name_lower.as_str()) {
                out_headers.insert(name, value.clone());
            }
        }
        // 注入网关头
        if let Ok(v) = trace_id.parse() {
            out_headers.insert("x-trace-id", v);
        }
        // 9c. XFF 注入：使用真实客户端 IP
        if let Ok(v) = real_ip.parse() {
            out_headers.insert("x-forwarded-for", v);
        }
        if let Ok(v) = "http".parse() {
            out_headers.insert("x-forwarded-proto", v);
        }
        if let Ok(v) = request_id.parse() {
            out_headers.insert("x-request-id", v);
        }
        // Host 头重写
        let host_value = route
            .host_header
            .as_deref()
            .unwrap_or(&node.address);
        if let Ok(v) = host_value.parse() {
            out_headers.insert(http::header::HOST, v);
        }

        *upstream_req.headers_mut() = out_headers;

        // 10. 调用 proxy 实际转发到上游（含重试）
        let method_str = method_clone.as_str();
        let is_idempotent = matches!(method_str, "GET" | "HEAD" | "OPTIONS");
        let can_retry = is_idempotent || route.allow_retry_non_idempotent;

        let mut proxy_result = Err(ConrogateError::UpstreamConnectFailed("no attempt".into()));
        let saved_headers = upstream_req.headers().clone();
        let full_body = upstream_req.into_body();
        let body_bytes: Bytes = http_body_util::BodyExt::collect(full_body)
            .await
            .map_err(|e| ConrogateError::UpstreamConnectFailed(format!("body collect: {e}")))?
            .to_bytes()
            .into();

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                if !can_retry {
                    break;
                }
                // 指数退避 + 抖动
                let backoff = std::time::Duration::from_millis(
                    (1u64 << attempt) * 10 + (uuid::Uuid::new_v4().as_u128() % 50) as u64,
                );
                tokio::time::sleep(backoff).await;
                tracing::warn!(attempt, route_id = route.id, "retrying request");
            }

            // 每次重试重建请求（body 已 clone）
            let mut retry_req = Request::builder()
                .method(method_clone.clone())
                .uri(upstream_uri_clone.clone())
                .body(body_from_bytes(body_bytes.clone()))
                .map_err(|e| ConrogateError::UpstreamConnectFailed(format!("request build: {e}")))?;
            *retry_req.headers_mut() = saved_headers.clone();

            proxy_result = crate::proxy::forward_http(
                &self.client,
                &node,
                retry_req,
                self.timeout,
            )
            .await;

            match &proxy_result {
                Ok(r) => {
                    // 5xx 可重试
                    if r.status.as_u16() >= 500 && can_retry && attempt < self.max_retries {
                        continue;
                    }
                    break;
                }
                Err(e) => {
                    // 连接失败/超时可重试
                    let retryable = matches!(e,
                        ConrogateError::UpstreamTimeout |
                        ConrogateError::UpstreamConnectFailed(_)
                    );
                    if retryable && can_retry && attempt < self.max_retries {
                        continue;
                    }
                    break;
                }
            }
        }

        // 11. 记录结果（成功/失败反馈给熔断器）
        let success = proxy_result.is_ok();
        self.svc
            .traffic
            .record_result(route.id, node.id, success)
            .await;
        // 请求完成，释放节点（LeastConnections 递减计数）
        self.svc.balancer.release_node(&route, &node).await;

        let proxy_result = proxy_result?;

        // 12. 构造响应
        let mut resp_builder = Response::builder().status(proxy_result.status);
        if let Some(h) = resp_builder.headers_mut() {
            *h = proxy_result.headers.clone();
        }

        // 12a. 响应方向注入头
        let out_headers = resp_builder.headers_mut().unwrap();
        if let Ok(v) = trace_id.parse() {
            out_headers.insert("x-trace-id", v);
        }
        if let Ok(v) = request_id.parse() {
            out_headers.insert("x-request-id", v);
        }

        let resp = resp_builder
            .body(proxy_result.body)
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Bytes::new())
                    .unwrap()
            });

        // 13. 插件 after_response
        let mut plugin_resp = PluginResponse {
            status: proxy_result.status.as_u16(),
            headers: proxy_result.headers,
            body: resp.body().clone(),
        };

        self.svc
            .plugins
            .execute_after_response(&mut plugin_ctx, &mut plugin_resp, &plugins)
            .await?;

        // 14. 遥测：记录指标（含实际延迟）
        let is_2xx = proxy_result.status.as_u16() >= 200 && proxy_result.status.as_u16() < 300;
        let is_4xx = proxy_result.status.as_u16() >= 400 && proxy_result.status.as_u16() < 500;
        let is_5xx = proxy_result.status.as_u16() >= 500;
        let latency_ms = start.elapsed().as_millis() as f64;

        self.svc.telemetry.record_metric(
            conrogate_contract::dto::MetricRow {
                ts: chrono::Utc::now(),
                bucket_sec: 10,
                route_id: Some(route.id),
                gate_id: String::new(),
                qps: 1,
                total_requests: 1,
                avg_latency_ms: latency_ms,
                p50_ms: latency_ms as u32,
                p90_ms: latency_ms as u32,
                p99_ms: latency_ms as u32,
                status_2xx: if is_2xx { 1 } else { 0 },
                status_3xx: 0,
                status_4xx: if is_4xx { 1 } else { 0 },
                status_5xx: if is_5xx { 1 } else { 0 },
                sessions: 0,
                bytes_in: 0,
                bytes_out: 0,
            }
        ).await;

        Ok(resp)
    }

    /// 流式处理 HTTP 请求 — 不缓冲 body，直接透传到上游。
    /// 适用于路由无 requires_body 插件的场景（大文件上传等）。
    /// 路由已由 HyperServiceBridge 预匹配，不重试（body 不可 clone）。
    async fn handle_stream(
        &self,
        parts: http::request::Parts,
        body: hyper::body::Incoming,
        route: conrogate_contract::dto::RouteSnapshot,
        client_ip: String,
    ) -> Result<Response<Bytes>, ConrogateError> {
        let method = parts.method.clone();
        let uri = parts.uri.clone();
        let headers = parts.headers.clone();
        let match_info = RouteMatchInfo::from_http_request(&method, &uri, &headers);
        let request_id = uuid::Uuid::new_v4().to_string();
        let trace_id = headers
            .get("x-trace-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(&request_id)
            .to_string();

        // XFF 信任链
        let real_ip = self.resolve_real_ip(&client_ip, &headers);

        // 记录请求开始时间（用于延迟统计）
        let start = std::time::Instant::now();

        // 构造插件上下文（body = None：流式模式不将 body 载入内存）
        let mut plugin_ctx = PluginContext {
            request_id: request_id.clone(),
            trace_id: trace_id.clone(),
            route_id: route.id,
            client_ip: real_ip.clone(),
            protocol: ProtocolId::Http,
            http: Some(HttpContext {
                method: method.clone(),
                path: match_info.path.clone(),
                query: match_info
                    .query_params
                    .iter()
                    .cloned()
                    .collect(),
                headers: headers.clone(),
                body: None,
            }),
            tunnel: None,
            services: conrogate_contract::plugin::PluginServices {
                metrics: Arc::new(NoopMetrics),
                logger: Arc::new(NoopLogger),
            },
        };

        // 执行插件 before_request
        let plugins = self.resolve_plugins(&route.plugin_chain);
        let plugin_outcome = self
            .svc
            .plugins
            .execute_before_request(&mut plugin_ctx, &plugins)
            .await?;

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

        // 流量治理检查
        if let Err(e) = self.svc
            .traffic
            .check_rate_limit(route.id, &real_ip)
            .await
        {
            // 上报限流事件到遥测
            self.svc.telemetry.record_event(
                conrogate_contract::dto::EventRow {
                    ts: chrono::Utc::now(),
                    event_type: "rate_limited".into(),
                    route_id: Some(route.id),
                    upstream_id: None,
                    trace_id: Some(trace_id.clone()),
                    detail: serde_json::json!({
                        "client_ip": real_ip,
                        "reason": e.to_string(),
                    }),
                }
            ).await;
            return Err(e);
        }

        // 选择上游节点
        let node = self.svc.balancer.select_upstream(&route, Some(&real_ip)).await?;

        // 熔断检查
        self.svc
            .traffic
            .check_circuit_breaker(route.id, node.id)
            .await?;

        // WebSocket 升级检测（路由匹配 + 上游选择完成后）
        let upgrade_check_req = Request::builder()
            .method(method.clone())
            .uri(uri.clone())
            .body(Bytes::new())
            .unwrap();
        if crate::upgrade::is_upgrade_request(&upgrade_check_req) {
            let mut resp = crate::upgrade::build_upgrade_response(&upgrade_check_req);
            if let Ok(v) = node.address.parse() {
                resp.headers_mut().insert("X-WS-Upstream-Addr", v);
            }
            if let Ok(v) = trace_id.parse() {
                resp.headers_mut().insert("X-WS-Trace-Id", v);
            }
            tracing::info!(
                trace_id = %trace_id,
                upstream = %node.address,
                "websocket upgrade request (stream), returning 101 with upstream addr"
            );
            return Ok(resp);
        }

        // 构造上游请求（流式 body）
        let path_and_query = uri
            .path_and_query()
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| "/".to_string());
        let upstream_addr = format!("http://{}", node.address);
        let upstream_uri: http::Uri = format!("{}{}", upstream_addr, path_and_query)
            .parse()
            .map_err(|e| ConrogateError::UpstreamConnectFailed(format!("uri parse: {e}")))?;

        let mut upstream_req = Request::builder()
            .method(method)
            .uri(upstream_uri)
            .body(body_from_incoming(body))
            .map_err(|e| ConrogateError::UpstreamConnectFailed(format!("request build: {e}")))?;

        // Header 处理
        let mut out_headers = http::HeaderMap::new();
        for (name, value) in headers.iter() {
            let name_lower = name.as_str().to_lowercase();
            if !SENSITIVE_HEADERS.contains(&name_lower.as_str()) {
                out_headers.insert(name, value.clone());
            }
        }
        if let Ok(v) = trace_id.parse() {
            out_headers.insert("x-trace-id", v);
        }
        if let Ok(v) = real_ip.parse() {
            out_headers.insert("x-forwarded-for", v);
        }
        if let Ok(v) = "http".parse() {
            out_headers.insert("x-forwarded-proto", v);
        }
        if let Ok(v) = request_id.parse() {
            out_headers.insert("x-request-id", v);
        }
        let host_value = route
            .host_header
            .as_deref()
            .unwrap_or(&node.address);
        if let Ok(v) = host_value.parse() {
            out_headers.insert(http::header::HOST, v);
        }
        *upstream_req.headers_mut() = out_headers;

        // 流式转发（不重试：body 不可 clone）
        let proxy_result = crate::proxy::forward_http_stream(
            &self.client,
            &node,
            upstream_req,
            self.timeout,
        )
        .await;

        // 记录结果
        let success = proxy_result.is_ok();
        self.svc
            .traffic
            .record_result(route.id, node.id, success)
            .await;
        // 请求完成，释放节点（LeastConnections 递减计数）
        self.svc.balancer.release_node(&route, &node).await;

        let proxy_result = proxy_result?;

        // 构造响应
        let mut resp_builder = Response::builder().status(proxy_result.status);
        if let Some(h) = resp_builder.headers_mut() {
            *h = proxy_result.headers.clone();
        }
        let out_headers = resp_builder.headers_mut().unwrap();
        if let Ok(v) = trace_id.parse() {
            out_headers.insert("x-trace-id", v);
        }
        if let Ok(v) = request_id.parse() {
            out_headers.insert("x-request-id", v);
        }

        let resp = resp_builder
            .body(proxy_result.body)
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Bytes::new())
                    .unwrap()
            });

        // 插件 after_response
        let mut plugin_resp = PluginResponse {
            status: proxy_result.status.as_u16(),
            headers: proxy_result.headers,
            body: resp.body().clone(),
        };

        self.svc
            .plugins
            .execute_after_response(&mut plugin_ctx, &mut plugin_resp, &plugins)
            .await?;

        // 遥测（含实际延迟）
        let is_2xx = proxy_result.status.as_u16() >= 200 && proxy_result.status.as_u16() < 300;
        let is_4xx = proxy_result.status.as_u16() >= 400 && proxy_result.status.as_u16() < 500;
        let is_5xx = proxy_result.status.as_u16() >= 500;
        let latency_ms = start.elapsed().as_millis() as f64;

        self.svc.telemetry.record_metric(
            conrogate_contract::dto::MetricRow {
                ts: chrono::Utc::now(),
                bucket_sec: 10,
                route_id: Some(route.id),
                gate_id: String::new(),
                qps: 1,
                total_requests: 1,
                avg_latency_ms: latency_ms,
                p50_ms: latency_ms as u32,
                p90_ms: latency_ms as u32,
                p99_ms: latency_ms as u32,
                status_2xx: if is_2xx { 1 } else { 0 },
                status_3xx: 0,
                status_4xx: if is_4xx { 1 } else { 0 },
                status_5xx: if is_5xx { 1 } else { 0 },
                sessions: 0,
                bytes_in: 0,
                bytes_out: 0,
            }
        ).await;

        Ok(resp)
    }
}

#[async_trait::async_trait]
impl ProtocolHandler for HttpProtocolHandler {
    fn protocol(&self) -> ProtocolId {
        ProtocolId::Http
    }

    async fn handle_http(
        &self,
        req: Request<Bytes>,
        client_ip: String,
    ) -> Result<Response<Bytes>, ConrogateError> {
        self.handle(req, client_ip).await
    }

    async fn handle_http_stream(
        &self,
        parts: http::request::Parts,
        body: hyper::body::Incoming,
        route: conrogate_contract::dto::RouteSnapshot,
        client_ip: String,
    ) -> Result<Response<Bytes>, ConrogateError> {
        self.handle_stream(parts, body, route, client_ip).await
    }
}
