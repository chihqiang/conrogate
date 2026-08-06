//! 协议 Handler 抽象：入站协议处理 Trait + 注册表。
//!
//! 扩展新协议时实现 `ProtocolHandler` Trait 并注册到 `ProtocolHandlerRegistry`，
//! 网关侧按 `ProtocolId` 查找对应 handler 分发处理，无需修改网关核心。

use bytes::Bytes;
use crate::contract::dto::RouteSnapshot;
use crate::contract::protocol::ProtocolId;
use crate::contract::ConrogateError;
use http::{Request, Response};
use hyper::body::Incoming;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::net::TcpStream;

/// 入站协议 Handler：每种入站协议实现一个 handler。
///
/// 各协议只实现自己需要的方法，其余方法沿用默认实现（返回 `ProtocolNotSupported`）。
#[async_trait::async_trait]
pub trait ProtocolHandler: Send + Sync {
    /// 协议标识
    fn protocol(&self) -> ProtocolId;

    /// HTTP 请求处理（缓冲模式：body 已载入内存）
    async fn handle_http(
        &self,
        _req: Request<Bytes>,
        _client_ip: String,
    ) -> Result<Response<Bytes>, ConrogateError> {
        Err(ConrogateError::ProtocolNotSupported(
            self.protocol().to_string(),
        ))
    }

    /// HTTP 请求处理（流式模式：请求体与响应体均不缓冲，直接透传）
    async fn handle_http_stream(
        &self,
        _parts: http::request::Parts,
        _body: Incoming,
        _route: RouteSnapshot,
        _client_ip: String,
    ) -> Result<Response<crate::protocol::proxy::ReqBody>, ConrogateError> {
        Err(ConrogateError::ProtocolNotSupported(
            self.protocol().to_string(),
        ))
    }

    /// TCP 隧道处理（原始字节流转发）
    async fn handle_tcp(
        &self,
        _listen_addr: String,
        _sni: Option<String>,
        _client_ip: String,
        _stream: TcpStream,
    ) -> Result<(), ConrogateError> {
        Err(ConrogateError::ProtocolNotSupported(
            self.protocol().to_string(),
        ))
    }
}

/// 协议 Handler 注册表：按 `ProtocolId` 注册 / 查找 handler。
#[derive(Default)]
pub struct ProtocolHandlerRegistry {
    handlers: RwLock<HashMap<ProtocolId, Arc<dyn ProtocolHandler>>>,
}

impl ProtocolHandlerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册协议 handler
    pub fn register(&self, handler: Arc<dyn ProtocolHandler>) {
        let protocol = handler.protocol();
        self.handlers.write().unwrap().insert(protocol, handler);
        tracing::info!(protocol = %protocol, "protocol handler registered");
    }

    /// 按协议查找 handler
    pub fn get(&self, protocol: ProtocolId) -> Option<Arc<dyn ProtocolHandler>> {
        self.handlers.read().unwrap().get(&protocol).cloned()
    }

    /// 列出已注册协议
    pub fn protocols(&self) -> Vec<ProtocolId> {
        self.handlers.read().unwrap().keys().cloned().collect()
    }
}

// ── 插件服务适配器：把插件可访问的指标/日志转发到真实遥测管线 ──

/// 插件指标 → 遥测事件（走聚合落库链路）
pub(crate) struct TelemetryPluginMetrics {
    telemetry: Arc<dyn crate::contract::gateway::TelemetryReport>,
}

#[async_trait::async_trait]
impl crate::contract::plugin::PluginMetrics for TelemetryPluginMetrics {
    async fn increment(&self, name: &str) {
        self.telemetry
            .record_event(crate::contract::dto::EventRow {
                ts: chrono::Utc::now(),
                event_type: "plugin.metric.increment".into(),
                route_id: None,
                upstream_id: None,
                trace_id: None,
                detail: serde_json::json!({ "name": name, "value": 1 }),
            })
            .await;
    }

    async fn gauge(&self, name: &str, value: f64) {
        self.telemetry
            .record_event(crate::contract::dto::EventRow {
                ts: chrono::Utc::now(),
                event_type: "plugin.metric.gauge".into(),
                route_id: None,
                upstream_id: None,
                trace_id: None,
                detail: serde_json::json!({ "name": name, "value": value }),
            })
            .await;
    }
}

/// 插件日志 → 遥测事件 + tracing（结构化日志，挂载当前 span）
pub(crate) struct TelemetryPluginLogger {
    telemetry: Arc<dyn crate::contract::gateway::TelemetryReport>,
}

#[async_trait::async_trait]
impl crate::contract::plugin::PluginLogger for TelemetryPluginLogger {
    async fn log(&self, level: &str, message: &str) {
        match level.to_ascii_lowercase().as_str() {
            "error" => tracing::error!(message),
            "warn" | "warning" => tracing::warn!(message),
            "debug" => tracing::debug!(message),
            "trace" => tracing::trace!(message),
            _ => tracing::info!(message),
        }
        self.telemetry
            .record_event(crate::contract::dto::EventRow {
                ts: chrono::Utc::now(),
                event_type: "plugin.log".into(),
                route_id: None,
                upstream_id: None,
                trace_id: None,
                detail: serde_json::json!({ "level": level, "message": message }),
            })
            .await;
    }
}

/// 从 ServiceContext 构造插件服务（注入真实遥测，替换 Noop 占位）
pub(crate) fn plugin_services(
    svc: &crate::contract::gateway::ServiceContext,
) -> crate::contract::plugin::PluginServices {
    crate::contract::plugin::PluginServices {
        metrics: Arc::new(TelemetryPluginMetrics {
            telemetry: svc.telemetry.clone(),
        }),
        logger: Arc::new(TelemetryPluginLogger {
            telemetry: svc.telemetry.clone(),
        }),
    }
}
