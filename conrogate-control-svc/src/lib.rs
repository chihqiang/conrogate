//! Conrogate 控制面服务：REST API、鉴权、审计、配置管理。

pub mod api;
pub mod auth;
pub mod audit;
pub mod handler;
pub mod openapi;
pub mod response;
pub mod service;

pub use api::build_router;
pub use handler::AppState;
pub use service::ControlService;
