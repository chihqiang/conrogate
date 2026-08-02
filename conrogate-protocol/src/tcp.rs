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
            client_ip,
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

        // 4. 选择上游
        let node = self.svc.balancer.select_upstream(&route).await?;

        // 5. 熔断检查
        self.svc
            .traffic
            .check_circuit_breaker(route.id, node.upstream_id)
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
            .record_result(route.id, node.upstream_id, node.id, success)
            .await;
        // 连接结束，释放节点（LeastConnections 递减计数）
        self.svc.balancer.release_node(&route, &node).await;

        // 8. 插件 on_disconnect
        let _ = self
            .svc
            .plugins
            .execute_on_disconnect(&mut plugin_ctx, &[])
            .await;

        result
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
