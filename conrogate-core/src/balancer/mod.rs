//! 负载均衡算法实现。
//!
//! 实现 `conrogate-contract` 中的 `LoadBalancer` Trait。

pub mod consistent_hash;
pub mod least_conn;
pub mod registry;
pub mod round_robin;
pub mod weighted;
