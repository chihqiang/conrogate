//! Conrogate 契约层：全局唯一接口定义。
//!
//! 包含全部公共 Trait、DTO、枚举、常量、错误类型。
//! 零内部依赖，仅依赖第三方库。

pub mod error;
pub mod protocol;
pub mod dto;
pub mod plugin;
pub mod balancer;
pub mod discovery;
pub mod health;
pub mod traffic;
pub mod gateway;
pub mod storage;
pub mod config;
pub mod constant;

pub use error::ConrogateError;
