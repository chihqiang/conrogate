//! Conrogate 契约层：全局唯一接口定义。
//!
//! 包含全部公共 Trait、DTO、枚举、常量、错误类型。
//! 零内部依赖，仅依赖第三方库。

pub mod balancer;
pub mod config;
pub mod constant;
pub mod discovery;
pub mod dto;
pub mod error;
pub mod gateway;
pub mod health;
pub mod plugin;
pub mod protocol;
pub mod storage;
pub mod traffic;

pub use error::ConrogateError;
