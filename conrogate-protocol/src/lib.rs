//! Conrogate 协议适配层：入站协议 Handler 抽象 + 注册表 + 内置协议实现。
//!
//! 扩展新协议时：
//! 1. 在 `conrogate-contract::protocol::ProtocolId` 中追加协议标识；
//! 2. 在 `conrogate-protocol` 中实现 `ProtocolHandler` Trait（`src/handler.rs`）；
//! 3. 在网关装配处注册到 `ProtocolHandlerRegistry`。
//! 网关核心只通过 `ProtocolHandlerRegistry` 分发，无需感知具体协议实现。

pub mod handler;
pub mod http;
pub mod tcp;
pub mod proxy;
pub mod upgrade;
pub mod dns;

pub use handler::{ProtocolHandler, ProtocolHandlerRegistry};
pub use http::HttpProtocolHandler;
pub use tcp::TcpTunnelProtocolHandler;
