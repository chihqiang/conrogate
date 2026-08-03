//! 协议 Handler 抽象：入站协议处理 Trait + 注册表。
//!
//! 扩展新协议时实现 `ProtocolHandler` Trait 并注册到 `ProtocolHandlerRegistry`，
//! 网关侧按 `ProtocolId` 查找对应 handler 分发处理，无需修改网关核心。

use bytes::Bytes;
use conrogate_contract::dto::RouteSnapshot;
use conrogate_contract::protocol::ProtocolId;
use conrogate_contract::ConrogateError;
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
        Err(ConrogateError::ProtocolNotSupported(self.protocol().to_string()))
    }

    /// HTTP 请求处理（流式模式：请求体与响应体均不缓冲，直接透传）
    async fn handle_http_stream(
        &self,
        _parts: http::request::Parts,
        _body: Incoming,
        _route: RouteSnapshot,
        _client_ip: String,
    ) -> Result<Response<crate::proxy::ReqBody>, ConrogateError> {
        Err(ConrogateError::ProtocolNotSupported(self.protocol().to_string()))
    }

    /// TCP 隧道处理（原始字节流转发）
    async fn handle_tcp(
        &self,
        _listen_addr: String,
        _sni: Option<String>,
        _client_ip: String,
        _stream: TcpStream,
    ) -> Result<(), ConrogateError> {
        Err(ConrogateError::ProtocolNotSupported(self.protocol().to_string()))
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

// ── 空实现辅助类型（插件服务注入占位）──

pub(crate) struct NoopMetrics;

#[async_trait::async_trait]
impl conrogate_contract::plugin::PluginMetrics for NoopMetrics {
    async fn increment(&self, _name: &str) {}
    async fn gauge(&self, _name: &str, _value: f64) {}
}

pub(crate) struct NoopLogger;

#[async_trait::async_trait]
impl conrogate_contract::plugin::PluginLogger for NoopLogger {
    async fn log(&self, _level: &str, _message: &str) {}
}
