//! Conrogate 插件：框架 + 官方内置插件。
//!
//! 框架部分（`framework/`）实现插件注册表、管线执行器、链构建器。
//! 官方插件（`cors/`、`auth/`、`header_rewrite/`、`ip_allow_deny/`）以 Rust 模块内建于本 crate。

pub mod auth;
pub mod cors;
pub mod framework;
pub mod header_rewrite;
pub mod ip_allow_deny;

/// 注册全部官方插件到注册表
pub async fn register_all(registry: &framework::registry::PluginRegistryImpl) {
    use conrogate_core::plugin::Plugin;
    use std::sync::Arc;

    let cors: Arc<dyn Plugin> = Arc::new(cors::CorsPlugin::new());
    let auth: Arc<dyn Plugin> = Arc::new(auth::AuthPlugin::new());
    let hr: Arc<dyn Plugin> = Arc::new(header_rewrite::HeaderRewritePlugin::new());
    let ip: Arc<dyn Plugin> = Arc::new(ip_allow_deny::IpAllowDenyPlugin::new());

    registry.register(cors).await;
    registry.register(auth).await;
    registry.register(hr).await;
    registry.register(ip).await;
}

/// 返回全部官方插件实例（调用方自行注册 + init）
pub fn official_plugins() -> Vec<std::sync::Arc<dyn conrogate_core::plugin::Plugin>> {
    vec![
        std::sync::Arc::new(cors::CorsPlugin::new()),
        std::sync::Arc::new(auth::AuthPlugin::new()),
        std::sync::Arc::new(header_rewrite::HeaderRewritePlugin::new()),
        std::sync::Arc::new(ip_allow_deny::IpAllowDenyPlugin::new()),
    ]
}
