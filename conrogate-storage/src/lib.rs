//! Conrogate 持久化层：SeaORM Entity、迁移、仓储实现。
//!
//! 实现 `conrogate-contract` 中定义的全部仓储 Trait。

pub mod config_cache;
pub mod convert;
pub mod entity;
pub mod migration;
pub mod pool;
pub mod repository;
