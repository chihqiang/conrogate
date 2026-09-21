//! Conrogate 公共基础层：接口契约、DTO、配置、日志。
//!
//! 包含全部公共 Trait、DTO、枚举、常量、错误类型、配置结构与日志初始化。
//! 仅依赖第三方库，不依赖任何其他 conrogate crate。

pub mod contract;
pub mod logging;

pub use contract::ConrogateError;
