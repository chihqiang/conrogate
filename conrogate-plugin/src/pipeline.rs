//! 插件管线执行器：按顺序执行插件钩子链。

use conrogate_contract::plugin::{Plugin, PluginContext, PluginOutcome, PluginResponse};
use conrogate_contract::ConrogateError;
use std::sync::Arc;

pub struct PluginPipelineImpl;

impl PluginPipelineImpl {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PluginPipelineImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl conrogate_contract::gateway::PluginExecutor for PluginPipelineImpl {
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
