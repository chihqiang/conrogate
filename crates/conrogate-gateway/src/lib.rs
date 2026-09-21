//! Conrogate 网关引擎：路由匹配、连接池、健康检查、遥测、配置热加载。
//!
//! 组合依赖 `conrogate-balancer`、`conrogate-protocol`、`conrogate-traffic`、
//! `conrogate-storage`、`conrogate-security` 组装数据面核心。

pub mod discovery;
pub mod filter;
pub mod health;
pub mod health_check;
pub mod pool;
pub mod route;
pub mod security;
pub mod server;
pub mod task_manager;
pub mod telemetry;
pub mod tls;
