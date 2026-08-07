//! Conrogate 契约层：全局唯一接口定义。
//!
//! 包含全部公共 Trait、DTO、枚举、常量、错误类型。
//! 仅依赖第三方库，作为 `conrogate-core` 的一个子模块。

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
pub mod response;
pub mod storage;
pub mod traffic;

pub use error::ConrogateError;
