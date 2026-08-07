//! Conrogate 核心层：契约、负载均衡、插件框架、协议适配、持久化、流量治理。
//!
//! 由原 `conrogate-contract` / `conrogate-balancer` / `conrogate-plugin` /
//! `conrogate-protocol` / `conrogate-storage` / `conrogate-traffic` 六个 crate 合并而成。

pub mod contract;
pub mod balancer;
pub mod control;
pub mod logging;
pub mod plugin;
pub mod protocol;
pub mod storage;
pub mod traffic;

pub use contract::ConrogateError;
