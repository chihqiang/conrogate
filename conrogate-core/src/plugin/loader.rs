//! 插件加载器：从注册表构建插件链。

use crate::contract::dto::PluginBindingDto;
use crate::contract::plugin::Plugin;
use crate::contract::ConrogateError;
use std::collections::HashMap;
use std::sync::Arc;

pub struct PluginLoader {
    registry: crate::plugin::registry::PluginRegistryImpl,
}

impl PluginLoader {
    pub fn new(registry: crate::plugin::registry::PluginRegistryImpl) -> Self {
        Self { registry }
    }

    /// 根据绑定配置构建有序插件链
    pub fn build_chain(
        &self,
        bindings: &[PluginBindingDto],
    ) -> Result<Vec<Arc<dyn Plugin>>, ConrogateError> {
        let mut sorted: Vec<&PluginBindingDto> = bindings.iter().filter(|b| b.enabled).collect();
        sorted.sort_by_key(|b| b.order);

        let mut chain: Vec<Arc<dyn Plugin>> = Vec::new();
        for binding in sorted {
            let Some(template) = self.registry.get(&binding.plugin_name) else {
                continue;
            };
            chain.push(template.configured(&binding.config)?);
        }
        Ok(chain)
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

/// 按 route_id 分组绑定并构建独立插件链（每绑定一个独立配置实例）。
///
/// 执行顺序按绑定的 `order` 升序；任一绑定实例化失败（config 非法）时
/// 返回 `Err`，由调用方决定回退当前配置，避免鉴权等阻断插件被静默跳过。
pub fn build_chains(
    registry: &crate::plugin::registry::PluginRegistryImpl,
    bindings: &[PluginBindingDto],
) -> Result<HashMap<u64, Vec<Arc<dyn Plugin>>>, ConrogateError> {
    let mut chains: HashMap<u64, Vec<Arc<dyn Plugin>>> = HashMap::new();

    let mut sorted: Vec<&PluginBindingDto> = bindings.iter().filter(|b| b.enabled).collect();
    sorted.sort_by_key(|b| b.order);

    for binding in sorted {
        let Some(template) = registry.get(&binding.plugin_name) else {
            tracing::warn!(
                route_id = binding.route_id,
                plugin = %binding.plugin_name,
                "plugin binding skipped: plugin not registered"
            );
            continue;
        };
        let instance = template.configured(&binding.config)?;
        chains.entry(binding.route_id).or_default().push(instance);
    }
    Ok(chains)
}
