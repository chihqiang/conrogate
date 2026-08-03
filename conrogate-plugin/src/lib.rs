//! Conrogate 插件框架：注册表、管线执行器、加载器。
//!
//! 实现 `conrogate-contract` 中的 `PluginRegistry` 和 `PluginPipeline` Trait。

pub mod loader;
pub mod pipeline;
pub mod registry;
