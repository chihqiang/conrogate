//! HTTP 协议处理器：完整转发链路（缓冲 / 流式两种模式）。

use crate::handler::{plugin_services, ProtocolHandler};
use crate::proxy::{body_from_bytes, body_from_incoming, HttpClient, ReqBody};
use bytes::Bytes;
use conrogate_contract::dto::{RouteSnapshot, UpstreamNodeDto};
use conrogate_contract::gateway::ServiceContext;
use conrogate_contract::plugin::{HttpContext, Plugin, PluginContext, PluginOutcome, PluginResponse};
use conrogate_contract::protocol::{ProtocolId, RouteMatchInfo};
use conrogate_contract::ConrogateError;
use http::{HeaderMap, Method, Request, Response, StatusCode, Uri, Version};
use hyper_util::client::legacy::Client;
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
    /// hyper 客户端（连接池复用，统一使用 BoxBody 体类型；支持 http/https 出站）
    client: HttpClient,
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
    /// 构建出站客户端（支持 http/https；skip_verify 跳过上游证书校验，仅非生产）
    fn build_client(skip_verify: bool) -> HttpClient {
        if skip_verify {
            tracing::warn!(
                "outbound TLS: skipping upstream certificate verification (non-production only)"
            );
            let verifier = Arc::new(crate::tls::NoVerifyServerCertVerifier);
            let tls_config = rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(verifier)
                .with_no_client_auth();
            let connector = hyper_rustls::HttpsConnectorBuilder::new()
                .with_tls_config(tls_config)
                .https_or_http();
            Client::builder(hyper_util::rt::TokioExecutor::new())
                .build(connector.enable_http1().enable_http2().build())
        } else {
            let connector = hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .expect("failed to load native TLS roots")
                .https_or_http();
            Client::builder(hyper_util::rt::TokioExecutor::new())
                .build(connector.enable_http1().enable_http2().build())
        }
    }

    pub fn new(svc: Arc<ServiceContext>) -> Self {
        let client = Self::build_client(false);
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
        let client = Self::build_client(false);
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
        let client = Self::build_client(false);
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

    /// 设置出站 TLS 跳过上游证书校验（重建客户端）
    pub fn with_outbound_tls(mut self, skip_verify: bool) -> Self {
        self.client = Self::build_client(skip_verify);
        self
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
    ///
    /// 仅信任来自可信代理的对端所携带的 XFF：
    /// - 无可信代理配置 → 直接使用 socket IP；
    /// - 对端不是可信代理（客户端直连）→ 忽略其自带的 XFF，防止伪造绕过 IP 限流；
    /// - 对端是可信代理 → 从右向左取第一个非可信 IP 作为真实客户端 IP。
    fn resolve_real_ip(&self, socket_ip: &str, headers: &http::HeaderMap) -> String {
        if self.trusted_proxies.is_empty() {
            return socket_ip.to_string();
        }

        // 对端不是可信代理：直连客户端可伪造 XFF，必须忽略
        if !self.is_trusted_proxy(socket_ip) {
            return socket_ip.to_string();
        }

        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            let chain: Vec<&str> = xff.split(',').map(|s| s.trim()).collect();
            for ip in chain.iter().rev() {
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
        let (parts, body) = req.into_parts();
        let method = parts.method;
        let uri = parts.uri;
        let headers = parts.headers;
        let meta = self.build_request_meta(&method, &uri, &headers, client_ip);

        let route = self
            .svc
            .routes
            .lookup_route(ProtocolId::Http, &meta.match_info)
            .await?
            .ok_or_else(|| ConrogateError::RouteNotFound(meta.match_info.path.clone()))?;

        match self
            .preflight(&meta, &method, &uri, parts.version, &headers, Some(&body), route)
            .await?
        {
            PreFlight::Terminate { code, body } => Ok(Response::builder()
                .status(code)
                .body(Bytes::from(body.to_string().into_bytes()))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Bytes::new())
                        .unwrap()
                })),
            PreFlight::WebSocketUpgrade {
                parts: resp_parts,
                upstream_addr,
            } => {
                tracing::info!(
                    trace_id = %meta.trace_id,
                    upstream = %upstream_addr,
                    "websocket upgrade request, returning 101 with upstream addr"
                );
                Ok(Response::from_parts(resp_parts, Bytes::new()))
            }
            PreFlight::Continue {
                mut plugin_ctx,
                plugins,
                route,
                node,
            } => {
                // 9. 构造上游请求（处理 Header）
                let upstream_uri = Self::build_upstream_uri(&node, &uri)?;
                let upstream_uri_clone = upstream_uri.clone();
                let method_clone = method.clone();
                let is_idempotent = matches!(method.as_str(), "GET" | "HEAD" | "OPTIONS");
                let can_retry = is_idempotent || route.allow_retry_non_idempotent;

                let mut upstream_req = Request::builder()
                    .method(method)
                    .uri(upstream_uri)
                    .body(body_from_bytes(body))
                    .map_err(|e| {
                        ConrogateError::UpstreamConnectFailed(format!("request build: {e}"))
                    })?;
                *upstream_req.headers_mut() = Self::build_out_headers(
                    &route,
                    &node,
                    &headers,
                    &meta.trace_id,
                    &meta.request_id,
                    &meta.real_ip,
                );

                // 10. 调用 proxy 实际转发到上游（含重试）
                let mut proxy_result =
                    Err(ConrogateError::UpstreamConnectFailed("no attempt".into()));
                let saved_headers = upstream_req.headers().clone();
                let full_body = upstream_req.into_body();
                let body_bytes: Bytes = http_body_util::BodyExt::collect(full_body)
                    .await
                    .map_err(|e| {
                        ConrogateError::UpstreamConnectFailed(format!("body collect: {e}"))
                    })?
                    .to_bytes();

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
                        .map_err(|e| {
                            ConrogateError::UpstreamConnectFailed(format!("request build: {e}"))
                        })?;
                    *retry_req.headers_mut() = saved_headers.clone();

                    proxy_result =
                        crate::proxy::forward_http(&self.client, &node, retry_req, self.timeout)
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
                            let retryable = matches!(
                                e,
                                ConrogateError::UpstreamTimeout
                                    | ConrogateError::UpstreamConnectFailed(_)
                            );
                            if retryable && can_retry && attempt < self.max_retries {
                                continue;
                            }
                            break;
                        }
                    }
                }

                // 11. 记录结果（成功/失败反馈给熔断器）
                self.record_outcome(&route, &node, proxy_result.is_ok()).await;
                if proxy_result.is_err() {
                    // 上游传输失败（超时/连接错误）：无响应体可返回，记录 5xx 指标
                    self.record_terminal_metric(&route, &meta, false, false, true, 0, 0)
                        .await;
                }
                let proxy_result = proxy_result?;

                let resp_body = proxy_result.body;
                let after_body = resp_body.clone();
                self.finalize_response(
                    &mut plugin_ctx,
                    &plugins,
                    &route,
                    proxy_result.status,
                    proxy_result.headers,
                    resp_body,
                    after_body,
                    &meta,
                )
                .await
            }
        }
    }

    /// 流式处理 HTTP 请求 — 请求体与响应体均不缓冲，直接透传上游。
    /// 适用于路由无 requires_body 插件的场景（大文件上传/下载、SSE 等）。
    /// 路由已由 HyperServiceBridge 预匹配，不重试（body 不可 clone）。
    async fn handle_stream(
        &self,
        parts: http::request::Parts,
        body: hyper::body::Incoming,
        route: RouteSnapshot,
        client_ip: String,
    ) -> Result<Response<ReqBody>, ConrogateError> {
        let method = parts.method.clone();
        let uri = parts.uri.clone();
        let headers = parts.headers.clone();
        let meta = self.build_request_meta(&method, &uri, &headers, client_ip);

        match self
            .preflight(&meta, &method, &uri, parts.version, &headers, None, route)
            .await?
        {
            PreFlight::Terminate { code, body } => Ok(Response::builder()
                .status(code)
                .body(body_from_bytes(Bytes::from(body.to_string().into_bytes())))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(body_from_bytes(Bytes::new()))
                        .unwrap()
                })),
            PreFlight::WebSocketUpgrade {
                parts: resp_parts,
                upstream_addr,
            } => {
                tracing::info!(
                    trace_id = %meta.trace_id,
                    upstream = %upstream_addr,
                    "websocket upgrade request (stream), returning 101 with upstream addr"
                );
                Ok(Response::from_parts(resp_parts, body_from_bytes(Bytes::new())))
            }
            PreFlight::Continue {
                mut plugin_ctx,
                plugins,
                route,
                node,
            } => {
                // 构造上游请求（流式 body）
                let upstream_uri = Self::build_upstream_uri(&node, &uri)?;
                let mut upstream_req = Request::builder()
                    .method(method)
                    .uri(upstream_uri)
                    .body(body_from_incoming(body))
                    .map_err(|e| {
                        ConrogateError::UpstreamConnectFailed(format!("request build: {e}"))
                    })?;
                *upstream_req.headers_mut() = Self::build_out_headers(
                    &route,
                    &node,
                    &headers,
                    &meta.trace_id,
                    &meta.request_id,
                    &meta.real_ip,
                );

                // 流式转发（不重试：body 不可 clone）
                let proxy_result =
                    crate::proxy::forward_http_stream(&self.client, &node, upstream_req, self.timeout)
                        .await;

                // 记录结果
                self.record_outcome(&route, &node, proxy_result.is_ok()).await;
                if proxy_result.is_err() {
                    // 上游传输失败：无响应体可返回，记录 5xx 指标
                    self.record_terminal_metric(&route, &meta, false, false, true, 0, 0)
                        .await;
                }
                let proxy_result = proxy_result?;

                self.finalize_response(
                    &mut plugin_ctx,
                    &plugins,
                    &route,
                    proxy_result.status,
                    proxy_result.headers,
                    body_from_incoming(proxy_result.body),
                    Bytes::new(),
                    &meta,
                )
                .await
            }
        }
    }

    /// 构造请求元数据：路由匹配信息、请求/追踪 ID、真实客户端 IP、开始时间
    fn build_request_meta(
        &self,
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        client_ip: String,
    ) -> RequestMeta {
        let match_info = RouteMatchInfo::from_http_request(method, uri, headers);
        let request_id = uuid::Uuid::new_v4().to_string();
        let trace_id = headers
            .get("x-trace-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(&request_id)
            .to_string();
        let real_ip = self.resolve_real_ip(&client_ip, headers);
        RequestMeta {
            match_info,
            request_id,
            trace_id,
            real_ip,
            start: std::time::Instant::now(),
        }
    }

    /// 前置流程（缓冲/流式共用）：插件 before_request → 限流 → 选节点 → 熔断 → WS 检测
    #[allow(clippy::too_many_arguments)]
    async fn preflight(
        &self,
        meta: &RequestMeta,
        method: &Method,
        uri: &Uri,
        version: Version,
        headers: &HeaderMap,
        plugin_body: Option<&Bytes>,
        route: RouteSnapshot,
    ) -> Result<PreFlight, ConrogateError> {
        // 3. 构造插件上下文
        let mut plugin_ctx = PluginContext {
            request_id: meta.request_id.clone(),
            trace_id: meta.trace_id.clone(),
            route_id: route.id,
            client_ip: meta.real_ip.clone(),
            protocol: ProtocolId::Http,
            http: Some(HttpContext {
                method: method.clone(),
                path: meta.match_info.path.clone(),
                query: meta.match_info.query_params.iter().cloned().collect(),
                headers: headers.clone(),
                body: plugin_body.cloned(),
            }),
            tunnel: None,
            services: plugin_services(&self.svc),
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
            return Ok(PreFlight::Terminate { code, body });
        }

        // 6. 流量治理检查（使用配置的 QPS）
        if let Err(e) = self
            .svc
            .traffic
            .check_rate_limit(route.id, &meta.real_ip)
            .await
        {
            // 上报限流事件到遥测
            self.svc
                .telemetry
                .record_event(conrogate_contract::dto::EventRow {
                    ts: chrono::Utc::now(),
                    event_type: "rate_limited".into(),
                    route_id: Some(route.id),
                    upstream_id: None,
                    trace_id: Some(meta.trace_id.clone()),
                    detail: serde_json::json!({
                        "client_ip": meta.real_ip,
                        "reason": e.to_string(),
                    }),
                })
                .await;
            // 限流请求无响应体可转发，直接记录 4xx 指标
            self.record_terminal_metric(&route, meta, false, true, false, 0, 0)
                .await;
            return Err(e);
        }

        // 7. 选择上游节点（一致性哈希按真实 client_ip）
        let node = self
            .svc
            .balancer
            .select_upstream(&route, Some(&meta.real_ip))
            .await?;

        // 8. 熔断检查
        if let Err(e) = self
            .svc
            .traffic
            .check_circuit_breaker(route.id, node.id)
            .await
        {
            // 熔断拒绝的请求不转发，记录 5xx 指标
            self.record_terminal_metric(&route, meta, false, false, true, 0, 0)
                .await;
            return Err(e);
        }

        // 8a. WebSocket 升级检测（路由匹配 + 上游选择完成后）
        let upgrade_check_req = Request::builder()
            .method(method.clone())
            .uri(uri.clone())
            .version(version)
            .body(Bytes::new())
            .unwrap();
        if crate::upgrade::is_upgrade_request(&upgrade_check_req) {
            let mut resp = crate::upgrade::build_upgrade_response(&upgrade_check_req);
            let upstream_addr = node.address.clone();
            // 设置上游地址头，供 HyperServiceBridge 提取并执行 WS 转发
            if let Ok(v) = upstream_addr.parse() {
                resp.headers_mut().insert("X-WS-Upstream-Addr", v);
            }
            if let Ok(v) = meta.trace_id.parse() {
                resp.headers_mut().insert("X-WS-Trace-Id", v);
            }
            let (parts, _) = resp.into_parts();
            return Ok(PreFlight::WebSocketUpgrade {
                parts,
                upstream_addr,
            });
        }

        Ok(PreFlight::Continue {
            plugin_ctx,
            plugins,
            route,
            node,
        })
    }

    /// 构造上游 URI（scheme + host + path_and_query）
    fn build_upstream_uri(node: &UpstreamNodeDto, uri: &Uri) -> Result<Uri, ConrogateError> {
        let path_and_query = uri
            .path_and_query()
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| "/".to_string());
        format!("{}{}", crate::proxy::upstream_addr(node), path_and_query)
            .parse()
            .map_err(|e| ConrogateError::UpstreamConnectFailed(format!("uri parse: {e}")))
    }

    /// 过滤敏感头 + 注入网关头（trace/request id、真实 IP、proto、Host）
    fn build_out_headers(
        route: &RouteSnapshot,
        node: &UpstreamNodeDto,
        headers: &HeaderMap,
        trace_id: &str,
        request_id: &str,
        real_ip: &str,
    ) -> HeaderMap {
        let mut out_headers = HeaderMap::new();
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
        if let Ok(v) = crate::proxy::upstream_scheme(node).parse() {
            out_headers.insert("x-forwarded-proto", v);
        }
        if let Ok(v) = request_id.parse() {
            out_headers.insert("x-request-id", v);
        }
        let host_value = route.host_header.as_deref().unwrap_or(&node.address);
        if let Ok(v) = host_value.parse() {
            out_headers.insert(http::header::HOST, v);
        }
        out_headers
    }

    /// 请求完成：反馈结果给熔断器 + 释放节点
    async fn record_outcome(&self, route: &RouteSnapshot, node: &UpstreamNodeDto, success: bool) {
        self.svc.traffic.record_result(route.id, node.id, success).await;
        // 请求完成，释放节点（LeastConnections 递减计数）
        self.svc.balancer.release_node(route, node).await;
    }

    /// 构造响应 + 注入响应头 + 插件 after_response + 遥测（缓冲/流式共用）
    #[allow(clippy::too_many_arguments)]
    async fn finalize_response<B>(
        &self,
        plugin_ctx: &mut PluginContext,
        plugins: &[Arc<dyn Plugin>],
        route: &RouteSnapshot,
        status: StatusCode,
        headers: HeaderMap,
        resp_body: B,
        after_body: Bytes,
        meta: &RequestMeta,
    ) -> Result<Response<B>, ConrogateError> {
        // 12. 构造响应
        let mut resp_builder = Response::builder().status(status);
        if let Some(h) = resp_builder.headers_mut() {
            *h = headers.clone();
        }
        // 12a. 响应方向注入头
        let out_headers = resp_builder.headers_mut().unwrap();
        if let Ok(v) = meta.trace_id.parse() {
            out_headers.insert("x-trace-id", v);
        }
        if let Ok(v) = meta.request_id.parse() {
            out_headers.insert("x-request-id", v);
        }

        let resp = match resp_builder.body(resp_body) {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!(error = %e, "response build failed");
                return Err(ConrogateError::UpstreamBadResponse(e.to_string()));
            }
        };

        // 13. 插件 after_response
        let mut plugin_resp = PluginResponse {
            status: status.as_u16(),
            headers,
            body: after_body,
        };
        self.svc
            .plugins
            .execute_after_response(plugin_ctx, &mut plugin_resp, plugins)
            .await?;

        // 14. 遥测：记录指标（含实际延迟）
        let is_2xx = status.as_u16() >= 200 && status.as_u16() < 300;
        let is_4xx = status.as_u16() >= 400 && status.as_u16() < 500;
        let is_5xx = status.as_u16() >= 500;
        self.record_terminal_metric(route, meta, is_2xx, is_4xx, is_5xx, 0, 0)
            .await;

        Ok(resp)
    }

    /// 上报终端指标（成功/失败共用）。
    ///
    /// 失败路径（限流、熔断、上游传输错误）没有可转发的响应体，
    /// 若不在此记录，错误请求在指标中完全不可观测。
    #[allow(clippy::too_many_arguments)]
    async fn record_terminal_metric(
        &self,
        route: &RouteSnapshot,
        meta: &RequestMeta,
        status_2xx: bool,
        status_4xx: bool,
        status_5xx: bool,
        bytes_in: u64,
        bytes_out: u64,
    ) {
        let latency_ms = meta.start.elapsed().as_millis() as f64;
        self.svc
            .telemetry
            .record_metric(conrogate_contract::dto::MetricRow {
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
                status_2xx: if status_2xx { 1 } else { 0 },
                status_3xx: 0,
                status_4xx: if status_4xx { 1 } else { 0 },
                status_5xx: if status_5xx { 1 } else { 0 },
                sessions: 0,
                bytes_in,
                bytes_out,
            })
            .await;
    }
}

/// 请求元数据（前置流程的公共输入）
struct RequestMeta {
    match_info: RouteMatchInfo,
    request_id: String,
    trace_id: String,
    real_ip: String,
    start: std::time::Instant,
}

/// 前置流程结果
#[allow(clippy::large_enum_variant)]
enum PreFlight {
    /// 正常继续转发
    Continue {
        plugin_ctx: PluginContext,
        plugins: Vec<Arc<dyn Plugin>>,
        route: RouteSnapshot,
        node: UpstreamNodeDto,
    },
    /// 插件终止请求
    Terminate { code: StatusCode, body: serde_json::Value },
    /// WebSocket 升级：返回 101 + 上游地址
    WebSocketUpgrade {
        parts: http::response::Parts,
        upstream_addr: String,
    },
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
    ) -> Result<Response<ReqBody>, ConrogateError> {
        self.handle_stream(parts, body, route, client_ip).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conrogate_contract::dto::{EventRow, MetricRow};
    use conrogate_contract::gateway::{
        PluginExecutor, RouteLookup, TelemetryReport, TrafficControl, UpstreamSelector,
    };
    use conrogate_contract::plugin::{PluginOutcome, PluginResponse};
    use http_body_util::{BodyExt, Full};
    use hyper::body::Incoming;
    use hyper_util::rt::TokioIo;
    use std::net::SocketAddr;

    // ── 测试桩 ──

    struct StubRoutes;
    #[async_trait::async_trait]
    impl RouteLookup for StubRoutes {
        async fn lookup_route(
            &self,
            _protocol: ProtocolId,
            _info: &RouteMatchInfo,
        ) -> Result<Option<RouteSnapshot>, ConrogateError> {
            Ok(Some(route_snapshot()))
        }
    }

    #[derive(Clone)]
    struct StubSelector {
        addr: String,
    }
    #[async_trait::async_trait]
    impl UpstreamSelector for StubSelector {
        async fn select_upstream(
            &self,
            _route: &RouteSnapshot,
            _key: Option<&str>,
        ) -> Result<UpstreamNodeDto, ConrogateError> {
            Ok(UpstreamNodeDto {
                id: 1,
                upstream_id: 1,
                address: self.addr.clone(),
                weight: 1,
                enabled: true,
            })
        }
    }

    struct StubTraffic;
    #[async_trait::async_trait]
    impl TrafficControl for StubTraffic {
        async fn check_rate_limit(&self, _route_id: u64, _client_ip: &str) -> Result<(), ConrogateError> {
            Ok(())
        }
        async fn check_circuit_breaker(&self, _route_id: u64, _node_id: u64) -> Result<(), ConrogateError> {
            Ok(())
        }
        async fn record_result(&self, _route_id: u64, _node_id: u64, _success: bool) {}
    }

    /// 恒限流的流量桩：验证限流失败路径指标
    struct FailingRateLimitTraffic;
    #[async_trait::async_trait]
    impl TrafficControl for FailingRateLimitTraffic {
        async fn check_rate_limit(&self, _route_id: u64, _client_ip: &str) -> Result<(), ConrogateError> {
            Err(ConrogateError::RateLimited)
        }
        async fn check_circuit_breaker(&self, _route_id: u64, _node_id: u64) -> Result<(), ConrogateError> {
            Ok(())
        }
        async fn record_result(&self, _route_id: u64, _node_id: u64, _success: bool) {}
    }

    /// 收集指标的遥测桩
    #[derive(Clone)]
    struct StubTelemetry {
        metrics: Arc<std::sync::Mutex<Vec<MetricRow>>>,
    }
    #[async_trait::async_trait]
    impl TelemetryReport for StubTelemetry {
        async fn record_metric(&self, metric: MetricRow) {
            self.metrics.lock().unwrap().push(metric);
        }
        async fn record_event(&self, _event: EventRow) {}
    }

    struct StubPlugins;
    #[async_trait::async_trait]
    impl PluginExecutor for StubPlugins {
        async fn execute_before_request(
            &self,
            _ctx: &mut PluginContext,
            _plugins: &[Arc<dyn Plugin>],
        ) -> Result<PluginOutcome, ConrogateError> {
            Ok(PluginOutcome::Continue)
        }
        async fn execute_after_response(
            &self,
            _ctx: &mut PluginContext,
            _resp: &mut PluginResponse,
            _plugins: &[Arc<dyn Plugin>],
        ) -> Result<(), ConrogateError> {
            Ok(())
        }
        async fn execute_on_connect(
            &self,
            _ctx: &mut PluginContext,
            _plugins: &[Arc<dyn Plugin>],
        ) -> Result<PluginOutcome, ConrogateError> {
            Ok(PluginOutcome::Continue)
        }
        async fn execute_on_disconnect(
            &self,
            _ctx: &mut PluginContext,
            _plugins: &[Arc<dyn Plugin>],
        ) -> Result<(), ConrogateError> {
            Ok(())
        }
    }

    fn route_snapshot() -> RouteSnapshot {
        RouteSnapshot {
            id: 1,
            protocol: ProtocolId::Http,
            upstream_id: Some(1),
            host_header: None,
            allow_retry_non_idempotent: false,
            plugin_chain: vec![],
            requires_body: true,
        }
    }

    fn make_handler(upstream_addr: SocketAddr) -> HttpProtocolHandler {
        make_handler_with(Some(upstream_addr), Arc::new(StubTraffic)).0
    }

    /// 构造 handler + 指标收集器（可自定义流量桩）
    fn make_handler_with(
        upstream: Option<SocketAddr>,
        traffic: Arc<dyn TrafficControl>,
    ) -> (HttpProtocolHandler, Arc<std::sync::Mutex<Vec<MetricRow>>>) {
        let metrics = Arc::new(std::sync::Mutex::new(Vec::new()));
        let telemetry = StubTelemetry {
            metrics: metrics.clone(),
        };
        let svc = Arc::new(ServiceContext {
            routes: Arc::new(StubRoutes),
            balancer: Arc::new(StubSelector {
                addr: upstream
                    .map(|a| a.to_string())
                    .unwrap_or_else(|| "127.0.0.1:1".to_string()),
            }),
            traffic,
            telemetry: Arc::new(telemetry),
            plugins: Arc::new(StubPlugins),
        });
        (HttpProtocolHandler::with_timeout(svc, Duration::from_secs(5)), metrics)
    }

    /// 上游回显服务器：请求体原样回显（响应带 x-upstream 头）
    async fn spawn_echo_upstream() -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let io = TokioIo::new(stream);
                tokio::spawn(async move {
                    let svc = hyper::service::service_fn(
                        |req: Request<Incoming>| async move {
                            let body = req.into_body().collect().await.unwrap().to_bytes();
                            let echo = format!("echo:{}", String::from_utf8_lossy(&body));
                            Ok::<_, std::convert::Infallible>(
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .header("x-upstream", "echo")
                                    .body(Full::new(Bytes::from(echo)))
                                    .unwrap(),
                            )
                        },
                    );
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, svc)
                        .await;
                });
            }
        });
        addr
    }

    /// 缓冲模式 handle：路由匹配 + 插件 + 限流 + 选节点 + 转发 + 遥测全链路
    #[tokio::test]
    async fn buffered_handle_forwards_to_upstream() {
        let upstream = spawn_echo_upstream().await;
        let handler = make_handler(upstream);

        let req = Request::builder()
            .method(Method::POST)
            .uri("http://gateway.local/echo")
            .body(Bytes::from_static(b"ping"))
            .unwrap();
        let resp = handler.handle(req, "192.168.1.10".into()).await.expect("handle ok");

        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body();
        assert_eq!(&body[..], b"echo:ping");
    }

    /// 缓冲模式响应注入 x-trace-id / x-request-id
    #[tokio::test]
    async fn buffered_handle_injects_response_headers() {
        let upstream = spawn_echo_upstream().await;
        let handler = make_handler(upstream);

        let req = Request::builder()
            .method(Method::GET)
            .uri("http://gateway.local/echo")
            .header("x-trace-id", "trace-123")
            .body(Bytes::new())
            .unwrap();
        let resp = handler.handle(req, "192.168.1.10".into()).await.expect("handle ok");

        assert!(resp.headers().contains_key("x-upstream"));
        assert_eq!(
            resp.headers().get("x-trace-id").unwrap().to_str().unwrap(),
            "trace-123"
        );
        assert!(resp.headers().contains_key("x-request-id"));
    }

    /// 上游连接失败：应上报 5xx 指标（失败路径可观测）
    #[tokio::test]
    async fn upstream_connect_failure_emits_5xx_metric() {
        // 指向未监听端口 127.0.0.1:1 → 连接拒绝
        let (handler, metrics) = make_handler_with(None, Arc::new(StubTraffic));

        let req = Request::builder()
            .method(Method::POST) // 非幂等：不重试，单次尝试
            .uri("http://gateway.local/echo")
            .body(Bytes::new())
            .unwrap();
        let result = handler.handle(req, "192.168.1.10".into()).await;
        assert!(result.is_err(), "connect to closed port should fail");

        let rows = metrics.lock().unwrap();
        assert_eq!(rows.len(), 1, "should emit exactly one metric row");
        assert_eq!(rows[0].status_5xx, 1);
        assert_eq!(rows[0].status_2xx, 0);
        assert_eq!(rows[0].status_4xx, 0);
        assert_eq!(rows[0].total_requests, 1);
    }

    /// 限流拒绝：应上报 4xx 指标
    #[tokio::test]
    async fn rate_limited_emits_4xx_metric() {
        let upstream = spawn_echo_upstream().await;
        let (handler, metrics) =
            make_handler_with(Some(upstream), Arc::new(FailingRateLimitTraffic));

        let req = Request::builder()
            .method(Method::GET)
            .uri("http://gateway.local/echo")
            .body(Bytes::new())
            .unwrap();
        let result = handler.handle(req, "192.168.1.10".into()).await;
        assert!(matches!(result, Err(ConrogateError::RateLimited)));

        let rows = metrics.lock().unwrap();
        assert_eq!(rows.len(), 1, "should emit exactly one metric row");
        assert_eq!(rows[0].status_4xx, 1);
        assert_eq!(rows[0].status_5xx, 0);
        assert_eq!(rows[0].status_2xx, 0);
    }

    /// 成功请求：应上报 2xx 指标
    #[tokio::test]
    async fn successful_request_emits_2xx_metric() {
        let upstream = spawn_echo_upstream().await;
        let (handler, metrics) = make_handler_with(Some(upstream), Arc::new(StubTraffic));

        let req = Request::builder()
            .method(Method::POST)
            .uri("http://gateway.local/echo")
            .body(Bytes::from_static(b"ping"))
            .unwrap();
        let resp = handler.handle(req, "192.168.1.10".into()).await.expect("handle ok");
        assert_eq!(resp.status(), StatusCode::OK);

        let rows = metrics.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status_2xx, 1);
        assert_eq!(rows[0].status_4xx, 0);
        assert_eq!(rows[0].status_5xx, 0);
    }

    /// 流式模式：通过真实 hyper 服务器（Incoming body）端到端转发
    #[tokio::test]
    async fn stream_handle_forwards_to_upstream() {
        let upstream = spawn_echo_upstream().await;
        let handler = Arc::new(make_handler(upstream));

        // 迷你网关服务器：将 Incoming 请求交给 handle_stream
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gw_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let handler = handler.clone();
                let io = TokioIo::new(stream);
                tokio::spawn(async move {
                    let svc = hyper::service::service_fn(move |req: Request<Incoming>| {
                        let handler = handler.clone();
                        async move {
                            let (parts, body) = req.into_parts();
                            let resp = handler
                                .handle_stream(parts, body, route_snapshot(), "192.168.1.10".into())
                                .await?;
                            Ok::<_, ConrogateError>(resp)
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, svc)
                        .await;
                });
            }
        });

        // hyper http1 客户端发送请求
        let stream = tokio::net::TcpStream::connect(gw_addr).await.unwrap();
        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("http://gateway.local/echo")
            .header("content-length", "4")
            .body(Full::new(Bytes::from_static(b"ping")))
            .unwrap();
        let resp = sender.send_request(req).await.expect("send request");
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"echo:ping");
    }
}
