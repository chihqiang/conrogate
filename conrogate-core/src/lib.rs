//! Conrogate 核心层：契约、负载均衡、插件框架、协议适配、持久化、流量治理、
//! 网关引擎、控制面服务。
//!
//! 由原 `conrogate-contract` / `conrogate-balancer` / `conrogate-plugin` /
//! `conrogate-protocol` / `conrogate-storage` / `conrogate-traffic` /
//! `conrogate-gateway` / `conrogate-control-svc` / 官方插件
//! `conrogate-plugin-{log,cors,auth}` 等 crate 合并而成。

pub mod balancer;
pub mod contract;
pub mod control;
pub mod gateway;
pub mod logging;
pub mod plugin;
pub mod plugins;
pub mod protocol;
pub mod storage;
pub mod traffic;

pub use contract::ConrogateError;
