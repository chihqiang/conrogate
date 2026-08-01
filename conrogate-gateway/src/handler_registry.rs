//! 协议 handler 注册表。

use conrogate_contract::gateway::ServiceContext;
use conrogate_contract::ConrogateError;
use std::sync::Arc;

/// 协议 handler 注册表
pub struct ProtocolHandlerRegistry {
    handlers: Vec<RegisteredHandler>,
}

struct RegisteredHandler {
    protocol: conrogate_contract::protocol::ProtocolId,
    handler: Arc<dyn ProtocolHandler>,
}

/// 协议 handler trait（内部定义，与文档 §5.1 对齐）
#[async_trait::async_trait]
pub trait ProtocolHandler: Send + Sync {
    /// 协议标识
    fn id(&self) -> conrogate_contract::protocol::ProtocolId;

    /// 处理入站 HTTP 请求
    async fn handle_http(
        &self,
        req: http::Request<bytes::Bytes>,
        client_ip: String,
        svc: &ServiceContext,
    ) -> Result<http::Response<bytes::Bytes>, ConrogateError>;
}

impl ProtocolHandlerRegistry {
    pub fn new() -> Self {
        Self { handlers: Vec::new() }
    }

    /// 注册 handler
    pub fn register(&mut self, handler: Arc<dyn ProtocolHandler>) {
        let protocol = handler.id();
        self.handlers.push(RegisteredHandler { protocol, handler });
    }

    /// 按协议查找 handler
    pub fn find(
        &self,
        protocol: conrogate_contract::protocol::ProtocolId,
    ) -> Option<Arc<dyn ProtocolHandler>> {
        self.handlers
            .iter()
            .find(|h| h.protocol == protocol)
            .map(|h| h.handler.clone())
    }
}

impl Default for ProtocolHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
