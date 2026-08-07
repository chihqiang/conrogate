//! 官方内置插件：访问日志、CORS 跨域、JWT 鉴权。
//!
//! 三个插件以 Rust 模块内建于核心 crate（不再作为独立包），
//! 由二进制装配后通过 `crate::gateway::server::GatewayServer::from_config*`
//! 注入网关，网关核心与插件共享同一 crate 但互不耦合具体实现。

pub mod auth;
pub mod cors;
pub mod log;
