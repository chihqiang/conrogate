//! Conrogate 公共基础层：接口契约、DTO、配置、日志。
//!
//! 包含全部公共 Trait、DTO、枚举、常量、错误类型、配置结构与日志初始化。
//! 仅依赖第三方库，不依赖任何其他 conrogate crate。

pub mod balancer;
pub mod config;
pub mod constant;
pub mod discovery;
pub mod dto;
pub mod error;
pub mod gateway;
pub mod health;
pub mod logging;
pub mod plugin;
pub mod protocol;
pub mod response;
pub mod storage;
pub mod traffic;

pub use error::ConrogateError;
