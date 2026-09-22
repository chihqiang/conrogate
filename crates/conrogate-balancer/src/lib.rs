//! Conrogate 负载均衡算法实现。
//!
//! 实现 `conrogate_core::balancer` 中的 `LoadBalancer` Trait。
//! 内置四种算法：轮询、加权轮询、最少连接、一致性哈希。

pub mod consistent_hash;
pub mod least_conn;
pub mod registry;
pub mod round_robin;
pub mod weighted;
