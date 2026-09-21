//! Conrogate 控制面 REST API：路由管理、上游管理、插件管理、配置版本、指标、审计。

pub mod api;
pub mod audit;
pub mod auth;
pub mod handler;
pub mod openapi;
pub mod service;
pub mod trace;

pub use api::build_router;
pub use handler::AppState;
pub use service::ControlService;
