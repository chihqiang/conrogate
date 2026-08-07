//! 网关核心：路由匹配、协议适配、代理转发、连接池。
//!
//! 协议适配层（HTTP/WebSocket/TCP 隧道）已抽离至 `crate::protocol`，
//! 网关通过 `ProtocolHandlerRegistry` 分发，扩展新协议无需修改网关核心。
//!
//! 本模块不依赖具体插件 crate：插件实例由二进制装配后通过
//! `GatewayServer::from_config*` 注入，核心仅持插件框架抽象。

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
