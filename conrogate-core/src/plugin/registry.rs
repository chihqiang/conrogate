//! 插件注册表实现。

use crate::contract::plugin::Plugin;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub struct PluginRegistryImpl {
    plugins: RwLock<HashMap<String, Arc<dyn Plugin>>>,
    bindings: RwLock<HashMap<u64, Vec<String>>>,
}

impl PluginRegistryImpl {
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            bindings: RwLock::new(HashMap::new()),
        }
    }

    /// 注册插件（静态插件）
    pub async fn register(&self, plugin: Arc<dyn Plugin>) {
        let name = plugin.name().to_string();
        let mut plugins = self.plugins.write().unwrap();
        plugins.insert(name, plugin);
    }

    /// 查找插件
    pub fn get(&self, name: &str) -> Option<Arc<dyn Plugin>> {
        self.plugins.read().unwrap().get(name).cloned()
    }

    /// 列出已注册插件名
    pub fn list_names(&self) -> Vec<String> {
        self.plugins.read().unwrap().keys().cloned().collect()
    }

    /// 列出全部已注册插件
    pub fn list_all(&self) -> Vec<Arc<dyn Plugin>> {
        self.plugins.read().unwrap().values().cloned().collect()
    }

    /// 返回 requires_body=true 的已注册插件名集合
    pub fn body_required_plugin_names(&self) -> std::collections::HashSet<String> {
        self.plugins
            .read()
            .unwrap()
            .iter()
            .filter(|(_, p)| p.requires_body())
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// 绑定插件到路由
    pub fn bind(&self, route_id: u64, plugin_name: &str) {
        let mut bindings = self.bindings.write().unwrap();
        bindings
            .entry(route_id)
            .or_default()
            .push(plugin_name.to_string());
    }

    /// 获取路由绑定的插件列表
    pub fn list_by_route(&self, route_id: u64) -> Vec<Arc<dyn Plugin>> {
        let bindings = self.bindings.read().unwrap();
        let plugins = self.plugins.read().unwrap();

        match bindings.get(&route_id) {
            Some(names) => names
                .iter()
                .filter_map(|name| plugins.get(name).cloned())
                .collect(),
            None => Vec::new(),
        }
    }

    /// 取消绑定
    pub fn unbind(&self, route_id: u64, plugin_name: &str) {
        let mut bindings = self.bindings.write().unwrap();
        if let Some(names) = bindings.get_mut(&route_id) {
            names.retain(|n| n != plugin_name);
        }
    }
}

impl Default for PluginRegistryImpl {
    fn default() -> Self {
        Self::new()
    }
}
