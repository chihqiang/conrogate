//! 插件加载器：从注册表构建插件链。

use crate::contract::dto::PluginBindingDto;
use crate::contract::plugin::Plugin;
use std::sync::Arc;

pub struct PluginLoader {
    registry: crate::plugin::registry::PluginRegistryImpl,
}

impl PluginLoader {
    pub fn new(registry: crate::plugin::registry::PluginRegistryImpl) -> Self {
        Self { registry }
    }

    /// 根据绑定配置构建有序插件链
    pub fn build_chain(&self, bindings: &[PluginBindingDto]) -> Vec<Arc<dyn Plugin>> {
        let mut chain: Vec<Arc<dyn Plugin>> = Vec::new();

        // 按 order 排序
        let mut sorted: Vec<&PluginBindingDto> = bindings.iter().filter(|b| b.enabled).collect();
        sorted.sort_by_key(|b| b.order);

        for binding in sorted {
            if let Some(plugin) = self.registry.get(&binding.plugin_name) {
                chain.push(plugin);
            }
        }

        chain
    }

    /// 重新加载配置
    pub fn reload(&self, bindings: &[PluginBindingDto]) {
        // 清除旧绑定
        // 注意：这里只是示例，实际实现需要更精细的 diff
        for binding in bindings {
            self.registry.bind(binding.route_id, &binding.plugin_name);
        }
    }
}
