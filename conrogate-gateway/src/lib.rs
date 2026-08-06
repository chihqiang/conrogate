//! Conrogate 网关核心：路由匹配、协议适配、代理转发、连接池。
//!
//! 协议适配层（HTTP/WebSocket/TCP 隧道）已抽离至 `conrogate-protocol` crate，
//! 网关通过 `ProtocolHandlerRegistry` 分发，扩展新协议无需修改网关核心。

pub mod discovery;
pub mod filter;
pub mod health;
pub mod health_check;
pub mod pool;
pub mod route;
pub mod server;
pub mod task_manager;
pub mod telemetry;
pub mod tls;
