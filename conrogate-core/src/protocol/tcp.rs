//! TCP 隧道协议处理器：原始字节流路由 + 转发。

use crate::contract::gateway::ServiceContext;
use crate::contract::plugin::{PluginContext, PluginOutcome};
use crate::contract::protocol::{ProtocolId, RouteMatchInfo};
use crate::contract::ConrogateError;
use crate::protocol::handler::{plugin_services, ProtocolHandler};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;

/// TCP 隧道协议处理器
pub struct TcpTunnelProtocolHandler {
    svc: Arc<ServiceContext>,
    timeout: Duration,
    /// 连接建立速率上限（0 = 不限制）
    conn_qps: u32,
    /// 每连接带宽上限 KB/s（0 = 不限制）
    bandwidth_kbps: u32,
}

impl TcpTunnelProtocolHandler {
    pub fn new(svc: Arc<ServiceContext>) -> Self {
        Self {
            svc,
            timeout: Duration::from_secs(30),
            conn_qps: 0,
            bandwidth_kbps: 0,
        }
    }

    /// 使用配置创建
    pub fn with_config(
        svc: Arc<ServiceContext>,
        timeout: Duration,
        conn_qps: u32,
        bandwidth_kbps: u32,
    ) -> Self {
        Self {
            svc,
            timeout,
            conn_qps,
            bandwidth_kbps,
        }
    }

    /// 获取路由当前生效的插件链（每绑定独立配置实例）
    fn resolve_plugins(&self, route_id: u64) -> Vec<Arc<dyn crate::contract::plugin::Plugin>> {
        self.svc.plugins.route_plugins(route_id)
    }

    /// 上报隧道建立前失败指标（限流/熔断/插件拒绝等）：
    /// 会话未建立（sessions=0），不记录则失败完全不可观测。
    async fn record_pre_tunnel_failure(&self, route_id: Option<u64>, status_4xx: bool) {
        self.svc
            .telemetry
            .record_metric(crate::contract::dto::MetricRow::raw_sample(
                chrono::Utc::now(),
                self.svc.gate_id.clone(),
                route_id,
                0.0,
                0,
                0,
                0,
                0,
                0,
                u64::from(status_4xx),
                u64::from(!status_4xx),
                0,
                0,
                0,
            ))
            .await;
    }

    /// 处理 TCP 隧道连接 — 完整转发链路
    async fn handle(
        &self,
        listen_addr: String,
        sni: Option<String>,
        client_ip: String,
        inbound: TcpStream,
    ) -> Result<(), ConrogateError> {
        let match_info = RouteMatchInfo::from_tunnel(&listen_addr, sni.as_deref());
        let start_ts = std::time::Instant::now();

        // 0. 全局 IP 黑名单：连接建立期按 socket IP 拦截
        if self.svc.blacklist.is_blocked(&client_ip) {
            tracing::info!(ip = %client_ip, "tcp tunnel rejected by global ip blacklist");
            self.record_pre_tunnel_failure(None, true).await;
            return Err(ConrogateError::Forbidden);
        }

        // 1. 路由匹配
        let route = match self
            .svc
            .routes
            .lookup_route(ProtocolId::TcpTunnel, &match_info)
            .await?
        {
            Some(route) => route,
            None => {
                self.record_pre_tunnel_failure(None, false).await;
                return Err(ConrogateError::RouteNotFound(listen_addr.clone()));
            }
        };

        // 2. 插件 on_connect
        let listen_port = listen_addr
            .rsplit_once(':')
            .and_then(|(_, p)| p.parse::<u16>().ok())
            .unwrap_or(0);
        let mut plugin_ctx = PluginContext {
            request_id: crate::contract::response::generate_trace_id(),
            trace_id: crate::contract::response::generate_trace_id(),
            route_id: route.id,
            client_ip: client_ip.clone(),
            protocol: ProtocolId::TcpTunnel,
            http: None,
            tunnel: Some(crate::contract::plugin::TunnelContext {
                remote_addr: listen_addr.clone(),
                sni: sni.clone(),
                alpn: None,
                listen_port,
            }),
            services: plugin_services(&self.svc),
            response_headers: http::HeaderMap::new(),
        };

        let plugin_outcome = self
            .svc
            .plugins
            .execute_on_connect(&mut plugin_ctx, &self.resolve_plugins(route.id))
            .await?;

        if let PluginOutcome::Terminate(code, _) = plugin_outcome {
            tracing::warn!(code = %code, "tcp tunnel rejected by plugin");
            self.record_pre_tunnel_failure(Some(route.id), false).await;
            return Err(ConrogateError::PluginRuntime(format!(
                "plugin rejected: {code}"
            )));
        }

        // 3. 流量治理
        if let Err(e) = self
            .svc
            .traffic
            .check_rate_limit(route.id, &plugin_ctx.client_ip)
            .await
        {
            self.record_pre_tunnel_failure(Some(route.id), true).await;
            return Err(e);
        }

        // 3a. 隧道连接建立速率限流
        if self.conn_qps > 0 {
            let conn_key = format!("conn:{listen_addr}");
            // 使用 traffic 模块的限流接口
            if let Err(e) = self.svc.traffic.check_rate_limit(u64::MAX, &conn_key).await {
                self.record_pre_tunnel_failure(Some(route.id), true).await;
                return Err(e);
            }
        }

        // 4. 选择上游（一致性哈希按 client_ip）
        let node = self
            .svc
            .balancer
            .select_upstream(&route, Some(&client_ip))
            .await?;

        // 5. 熔断检查
        if let Err(e) = self
            .svc
            .traffic
            .check_circuit_breaker(route.id, node.id)
            .await
        {
            self.record_pre_tunnel_failure(Some(route.id), false).await;
            return Err(e);
        }

        // 6. 实际转发
        tracing::info!(
            route_id = route.id,
            upstream = %node.address,
            "tcp tunnel established"
        );

        let max_bytes_per_sec = if self.bandwidth_kbps > 0 {
            Some((self.bandwidth_kbps as u64) * 1024)
        } else {
            None
        };
        let result =
            crate::protocol::proxy::forward_tcp(&node, inbound, self.timeout, max_bytes_per_sec)
                .await;

        // 7. 记录结果
        let success = result.is_ok();
        self.svc
            .traffic
            .record_result(route.id, node.id, success)
            .await;
        // 连接结束，释放节点（LeastConnections 递减计数）
        self.svc.balancer.release_node(&route, &node).await;

        // 7a. 隧道遥测：成功/失败都上报会话与字节数。
        // 失败会话无响应体，若不记录则在指标中完全不可观测。
        let (bytes_in, bytes_out) = match &result {
            Ok(stats) => (stats.bytes_in, stats.bytes_out),
            Err(_) => (0, 0),
        };
        self.svc
            .telemetry
            .record_metric(crate::contract::dto::MetricRow::raw_sample(
                chrono::Utc::now(),
                self.svc.gate_id.clone(),
                Some(route.id),
                start_ts.elapsed().as_millis() as f64,
                0,
                0,
                0,
                u64::from(success),
                0,
                0,
                u64::from(!success),
                1,
                bytes_in,
                bytes_out,
            ))
            .await;

        // 8. 插件 on_disconnect
        let _ = self
            .svc
            .plugins
            .execute_on_disconnect(&mut plugin_ctx, &self.resolve_plugins(route.id))
            .await;

        result.map(|_| ())
    }
}

#[async_trait::async_trait]
impl ProtocolHandler for TcpTunnelProtocolHandler {
    fn protocol(&self) -> ProtocolId {
        ProtocolId::TcpTunnel
    }

    async fn handle_tcp(
        &self,
        listen_addr: String,
        sni: Option<String>,
        client_ip: String,
        stream: TcpStream,
    ) -> Result<(), ConrogateError> {
        self.handle(listen_addr, sni, client_ip, stream).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::dto::{MetricRow, PluginBindingDto, RouteSnapshot, UpstreamNodeDto};
    use crate::contract::gateway::{
        RouteLookup, TelemetryReport, TrafficControl, UpstreamSelector,
    };
    use crate::contract::plugin::{Plugin, PluginKind};
    use crate::plugin::registry::PluginRegistryImpl;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // ── 测试桩 ──

    struct StubTcpRoutes;
    #[async_trait::async_trait]
    impl RouteLookup for StubTcpRoutes {
        async fn lookup_route(
            &self,
            _protocol: ProtocolId,
            _info: &RouteMatchInfo,
        ) -> Result<Option<RouteSnapshot>, ConrogateError> {
            Ok(Some(RouteSnapshot {
                id: 1,
                protocol: ProtocolId::TcpTunnel,
                upstream_id: Some(1),
                host_header: None,
                allow_retry_non_idempotent: false,
                ws_strip_sensitive_headers: false,
                plugin_chain: std::sync::Arc::new(vec![PluginBindingDto {
                    id: 1,
                    route_id: 1,
                    plugin_name: "tunnel-guard".to_string(),
                    config: serde_json::Value::Null,
                    order: 0,
                    blocking: true,
                    enabled: true,
                }]),
                requires_body: false,
            }))
        }
    }

    struct StubSelector;
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
                address: "127.0.0.1:1".to_string(),
                weight: 1,
                enabled: true,
            })
        }
    }

    struct StubTraffic;
    #[async_trait::async_trait]
    impl TrafficControl for StubTraffic {
        async fn check_rate_limit(
            &self,
            _route_id: u64,
            _client_ip: &str,
        ) -> Result<(), ConrogateError> {
            Ok(())
        }
        async fn check_circuit_breaker(
            &self,
            _route_id: u64,
            _node_id: u64,
        ) -> Result<(), ConrogateError> {
            Ok(())
        }
        async fn record_result(&self, _route_id: u64, _node_id: u64, _success: bool) {}
    }

    #[derive(Clone)]
    struct StubTelemetry {
        metrics: Arc<std::sync::Mutex<Vec<MetricRow>>>,
    }
    #[async_trait::async_trait]
    impl TelemetryReport for StubTelemetry {
        async fn record_metric(&self, metric: MetricRow) {
            self.metrics.lock().unwrap().push(metric);
        }
        async fn record_event(&self, _event: crate::contract::dto::EventRow) {}
    }

    /// 记录 on_connect/on_disconnect 调用次数的隧道插件
    struct TunnelGuardPlugin {
        connects: Arc<AtomicUsize>,
        disconnects: Arc<AtomicUsize>,
    }
    #[async_trait::async_trait]
    impl Plugin for TunnelGuardPlugin {
        fn name(&self) -> &'static str {
            "tunnel-guard"
        }
        fn kind(&self) -> PluginKind {
            PluginKind::Native
        }
        fn protocols(&self) -> &[ProtocolId] {
            &[ProtocolId::TcpTunnel]
        }
        fn is_blocking(&self) -> bool {
            true
        }
        fn validate_config(&self, _config: &serde_json::Value) -> Result<(), ConrogateError> {
            Ok(())
        }
        fn configured(
            &self,
            _config: &serde_json::Value,
        ) -> Result<std::sync::Arc<dyn Plugin>, ConrogateError> {
            Ok(std::sync::Arc::new(TunnelGuardPlugin {
                connects: self.connects.clone(),
                disconnects: self.disconnects.clone(),
            }))
        }
        async fn before_request(
            &self,
            _ctx: &mut PluginContext,
        ) -> Result<PluginOutcome, ConrogateError> {
            Ok(PluginOutcome::Continue)
        }
        async fn on_connect(
            &self,
            _ctx: &mut PluginContext,
        ) -> Result<PluginOutcome, ConrogateError> {
            self.connects.fetch_add(1, Ordering::SeqCst);
            Ok(PluginOutcome::Continue)
        }
        async fn on_disconnect(&self, _ctx: &mut PluginContext) -> Result<(), ConrogateError> {
            self.disconnects.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// 路由绑定插件链必须在隧道生命周期执行 on_connect/on_disconnect
    #[tokio::test]
    async fn tcp_tunnel_runs_route_bound_plugins() {
        let connects = Arc::new(AtomicUsize::new(0));
        let disconnects = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(PluginRegistryImpl::new());
        registry
            .register(Arc::new(TunnelGuardPlugin {
                connects: connects.clone(),
                disconnects: disconnects.clone(),
            }))
            .await;

        let metrics = Arc::new(std::sync::Mutex::new(Vec::new()));
        let executor = Arc::new(crate::plugin::pipeline::PluginPipelineImpl::new());
        let svc = Arc::new(ServiceContext {
            routes: Arc::new(StubTcpRoutes),
            balancer: Arc::new(StubSelector),
            traffic: Arc::new(StubTraffic),
            telemetry: Arc::new(StubTelemetry {
                metrics: metrics.clone(),
            }),
            plugins: executor.clone(),
            blacklist: Arc::new(crate::security::blacklist::BlacklistMatcher::new()),
            gate_id: "test-gate".into(),
        });

        let binding = PluginBindingDto {
            id: 1,
            route_id: 1,
            plugin_name: "tunnel-guard".into(),
            config: serde_json::Value::Null,
            order: 0,
            blocking: true,
            enabled: true,
        };
        let chains = crate::plugin::loader::build_chains(&registry, &[binding]).unwrap();
        executor.set_route_chains(chains);

        let handler = TcpTunnelProtocolHandler::with_config(svc, Duration::from_secs(2), 0, 0);

        // 入站连接（实际转发目标 127.0.0.1:1 无监听，将在转发阶段失败）
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let inbound = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let _server = listener.accept().await.unwrap();

        let result = handler
            .handle_tcp(
                "127.0.0.1:9000".to_string(),
                None,
                "127.0.0.1".to_string(),
                inbound,
            )
            .await;

        assert!(result.is_err(), "无监听上游应转发失败");
        assert_eq!(
            connects.load(Ordering::SeqCst),
            1,
            "on_connect 必须执行一次"
        );
        assert_eq!(
            disconnects.load(Ordering::SeqCst),
            1,
            "on_disconnect 必须执行一次"
        );
    }
}
