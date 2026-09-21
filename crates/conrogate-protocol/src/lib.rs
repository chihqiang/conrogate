//! Conrogate 协议适配层：入站协议 Handler 抽象 + 注册表 + 内置协议实现。
//!
//! 内置协议：HTTP/1.1 + HTTP/2、WebSocket（HTTP 升级）、TCP 隧道。
//! 扩展新协议时实现 `ProtocolHandler` Trait 并注册到 `ProtocolHandlerRegistry`。

pub mod dns;
pub mod handler;
pub mod http;
pub mod proxy;
pub mod tcp;
pub mod tls;
pub mod upgrade;

pub use handler::{ProtocolHandler, ProtocolHandlerRegistry};
pub use http::HttpProtocolHandler;
pub use tcp::TcpTunnelProtocolHandler;
