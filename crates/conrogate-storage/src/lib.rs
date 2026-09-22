//! Conrogate 数据模型与持久化层。
//!
//! 实现 `conrogate_core::storage` 中定义的全部仓储 Trait。
//! 包含 SeaORM Entity、迁移脚本、仓储实现、连接池、配置缓存。

pub mod config_cache;
pub mod convert;
pub mod entity;
pub mod migration;
pub mod pool;
pub mod repository;
pub mod seed;
