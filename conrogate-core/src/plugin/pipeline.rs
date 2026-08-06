//! 插件管线执行器：按顺序执行插件钩子链。
//! 支持热加载时通过 set_route_chain() 原子替换路由的插件链。

use crate::contract::plugin::{Plugin, PluginContext, PluginOutcome, PluginResponse};
use crate::contract::ConrogateError;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub struct PluginPipelineImpl {
    /// 路由插件链缓存（route_id → 插件链）
    /// 配置热加载时原子替换，存量请求使用旧链引用
    route_chains: RwLock<HashMap<u64, Vec<Arc<dyn Plugin>>>>,
}

impl PluginPipelineImpl {
    pub fn new() -> Self {
        Self {
            route_chains: RwLock::new(HashMap::new()),
        }
    }

    /// 热加载：原子替换路由的插件链
    /// 存量请求持有旧链的 Arc 引用，不受影响
    pub fn set_route_chain(&self, route_id: u64, plugins: Vec<Arc<dyn Plugin>>) {
        let mut chains = self.route_chains.write().unwrap();
        chains.insert(route_id, plugins);
        tracing::debug!(route_id, "plugin chain updated");
    }

    /// 批量更新路由插件链（配置热加载时调用）
    pub fn set_route_chains(&self, chains: HashMap<u64, Vec<Arc<dyn Plugin>>>) {
        let mut guard = self.route_chains.write().unwrap();
        *guard = chains;
        tracing::info!("all route plugin chains reloaded");
    }

    /// 获取路由的插件链（如缓存中有则返回缓存的，否则返回空）
    pub fn get_route_chain(&self, route_id: u64) -> Vec<Arc<dyn Plugin>> {
        let chains = self.route_chains.read().unwrap();
        chains.get(&route_id).cloned().unwrap_or_default()
    }
}

impl Default for PluginPipelineImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl crate::contract::gateway::PluginExecutor for PluginPipelineImpl {
    async fn execute_before_request(
        &self,
        ctx: &mut PluginContext,
        plugins: &[Arc<dyn Plugin>],
    ) -> Result<PluginOutcome, ConrogateError> {
        for plugin in plugins {
            // 检查协议匹配
            if !plugin.protocols().contains(&ctx.protocol) {
                continue;
            }

            let outcome = plugin.before_request(ctx).await?;
            match outcome {
                PluginOutcome::Continue => continue,
                PluginOutcome::Terminate(code, body) => {
                    return Ok(PluginOutcome::Terminate(code, body));
                }
            }
        }
        Ok(PluginOutcome::Continue)
    }

    async fn execute_after_response(
        &self,
        ctx: &mut PluginContext,
        resp: &mut PluginResponse,
        plugins: &[Arc<dyn Plugin>],
    ) -> Result<(), ConrogateError> {
        for plugin in plugins {
            if !plugin.protocols().contains(&ctx.protocol) {
                continue;
            }
            plugin.after_response(ctx, resp).await?;
        }
        Ok(())
    }

    async fn execute_on_connect(
        &self,
        ctx: &mut PluginContext,
        plugins: &[Arc<dyn Plugin>],
    ) -> Result<PluginOutcome, ConrogateError> {
        for plugin in plugins {
            if !plugin.protocols().contains(&ctx.protocol) {
                continue;
            }

            let outcome = plugin.on_connect(ctx).await?;
            match outcome {
                PluginOutcome::Continue => continue,
                PluginOutcome::Terminate(code, body) => {
                    return Ok(PluginOutcome::Terminate(code, body));
                }
            }
        }
        Ok(PluginOutcome::Continue)
    }

    async fn execute_on_disconnect(
        &self,
        ctx: &mut PluginContext,
        plugins: &[Arc<dyn Plugin>],
    ) -> Result<(), ConrogateError> {
        for plugin in plugins {
            if !plugin.protocols().contains(&ctx.protocol) {
                continue;
            }
            plugin.on_disconnect(ctx).await?;
        }
        Ok(())
    }
}
