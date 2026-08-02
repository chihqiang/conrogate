//! TCP 隧道协议处理器：原始字节流路由 + 转发。

use crate::handler::{NoopLogger, NoopMetrics, ProtocolHandler};
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
        let route = self
            .svc
            .routes
            .lookup_route(ProtocolId::TcpTunnel, &match_info)
            .await?
            .ok_or_else(|| ConrogateError::RouteNotFound(listen_addr.clone()))?;

        // 2. 插件 on_connect
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
                listen_port: 0,
            }),
            services: conrogate_contract::plugin::PluginServices {
                metrics: Arc::new(NoopMetrics),
                logger: Arc::new(NoopLogger),
            },
        };

        let plugin_outcome = self
            .svc
            .plugins
            .execute_on_connect(&mut plugin_ctx, &[])
            .await?;

        if let PluginOutcome::Terminate(code, _) = plugin_outcome {
            tracing::warn!(code = %code, "tcp tunnel rejected by plugin");
            return Err(ConrogateError::PluginRuntime(format!("plugin rejected: {code}")));
        }

        // 3. 流量治理
        self.svc
            .traffic
            .check_rate_limit(route.id, &plugin_ctx.client_ip)
            .await?;

        // 3a. 隧道连接建立速率限流
        if self.conn_qps > 0 {
            let conn_key = format!("conn:{listen_addr}");
            // 使用 traffic 模块的限流接口
            self.svc
                .traffic
                .check_rate_limit(u64::MAX, &conn_key)
                .await?;
        }

        // 4. 选择上游（一致性哈希按 client_ip）
        let node = self.svc.balancer.select_upstream(&route, Some(&client_ip)).await?;

        // 5. 熔断检查
        self.svc
            .traffic
            .check_circuit_breaker(route.id, node.id)
            .await?;

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
        let result = crate::proxy::forward_tcp(&node, inbound, self.timeout, max_bytes_per_sec).await;

        // 7. 记录结果
        let success = result.is_ok();
        self.svc
            .traffic
            .record_result(route.id, node.id, success)
            .await;
        // 连接结束，释放节点（LeastConnections 递减计数）
        self.svc.balancer.release_node(&route, &node).await;

        // 7a. 隧道遥测：记录会话数与字节数（docs/10 §2.4）
        if let Ok(stats) = &result {
            let duration_secs = start_ts.elapsed().as_secs().max(1);
            let sessions = if duration_secs > 0 { 1u64 } else { 0u64 };
            self.svc.telemetry.record_metric(
                conrogate_contract::dto::MetricRow {
                    ts: chrono::Utc::now(),
                    bucket_sec: 10,
                    route_id: Some(route.id),
                    gate_id: String::new(),
                    qps: sessions as u32,
                    total_requests: sessions,
                    avg_latency_ms: start_ts.elapsed().as_millis() as f64,
                    p50_ms: 0,
                    p90_ms: 0,
                    p99_ms: 0,
                    status_2xx: 0,
                    status_3xx: 0,
                    status_4xx: 0,
                    status_5xx: 0,
                    sessions,
                    bytes_in: stats.bytes_in,
                    bytes_out: stats.bytes_out,
                }
            ).await;
        }

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
