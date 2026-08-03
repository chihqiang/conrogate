//! TCP 隧道协议处理器：原始字节流路由 + 转发。

use crate::handler::{plugin_services, ProtocolHandler};
use conrogate_contract::gateway::ServiceContext;
use conrogate_contract::plugin::{PluginContext, PluginOutcome};
use conrogate_contract::protocol::{ProtocolId, RouteMatchInfo};
use conrogate_contract::ConrogateError;
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

    /// 上报隧道建立前失败指标（限流/熔断/插件拒绝等）：
    /// 会话未建立（sessions=0），不记录则失败完全不可观测。
    async fn record_pre_tunnel_failure(&self, route_id: Option<u64>, status_4xx: bool) {
        self.svc
            .telemetry
            .record_metric(conrogate_contract::dto::MetricRow::raw_sample(
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
            request_id: uuid::Uuid::new_v4().to_string(),
            trace_id: uuid::Uuid::new_v4().to_string(),
            route_id: route.id,
            client_ip: client_ip.clone(),
            protocol: ProtocolId::TcpTunnel,
            http: None,
            tunnel: Some(conrogate_contract::plugin::TunnelContext {
                remote_addr: listen_addr.clone(),
                sni: sni.clone(),
                alpn: None,
                listen_port,
            }),
            services: plugin_services(&self.svc),
        };

        let plugin_outcome = self
            .svc
            .plugins
            .execute_on_connect(&mut plugin_ctx, &[])
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
            if let Err(e) = self
                .svc
                .traffic
                .check_rate_limit(u64::MAX, &conn_key)
                .await
            {
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
            crate::proxy::forward_tcp(&node, inbound, self.timeout, max_bytes_per_sec).await;

        // 7. 记录结果
        let success = result.is_ok();
        self.svc
            .traffic
            .record_result(route.id, node.id, success)
            .await;
        // 连接结束，释放节点（LeastConnections 递减计数）
        self.svc.balancer.release_node(&route, &node).await;

        // 7a. 隧道遥测：成功/失败都上报会话与字节数（docs/10 §2.4）。
        // 失败会话无响应体，若不记录则在指标中完全不可观测。
        let (bytes_in, bytes_out) = match &result {
            Ok(stats) => (stats.bytes_in, stats.bytes_out),
            Err(_) => (0, 0),
        };
        self.svc
            .telemetry
            .record_metric(conrogate_contract::dto::MetricRow::raw_sample(
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
            .execute_on_disconnect(&mut plugin_ctx, &[])
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
