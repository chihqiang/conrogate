//! Conrogate 官方日志插件：请求访问日志记录。

use async_trait::async_trait;
use conrogate_core::contract::{
    plugin::{Plugin, PluginContext, PluginOutcome, PluginResponse},
    protocol::ProtocolId,
    ConrogateError,
};
use serde_json::Value;

/// 日志插件配置
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LogPluginConfig {
    pub log_body: bool,
    pub log_headers: bool,
    pub skip_paths: Vec<String>,
}

impl Default for LogPluginConfig {
    fn default() -> Self {
        Self {
            log_body: false,
            log_headers: false,
            skip_paths: vec!["/healthz".into(), "/readyz".into()],
        }
    }
}

/// 日志插件
pub struct LogPlugin {
    config: LogPluginConfig,
}

impl LogPlugin {
    pub fn new() -> Self {
        Self {
            config: LogPluginConfig::default(),
        }
    }
}

impl Default for LogPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for LogPlugin {
    fn name(&self) -> &'static str {
        "log"
    }

    fn kind(&self) -> conrogate_core::contract::plugin::PluginKind {
        conrogate_core::contract::plugin::PluginKind::Native
    }

    fn protocols(&self) -> &[ProtocolId] {
        &[ProtocolId::Http, ProtocolId::WebSocket]
    }

    fn is_blocking(&self) -> bool {
        false
    }

    fn validate_config(&self, config: &Value) -> Result<(), ConrogateError> {
        if config.is_null() {
            return Ok(());
        }
        serde_json::from_value::<LogPluginConfig>(config.clone())
            .map(|_| ())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))
    }

    async fn init(&self, config: &Value) -> Result<(), ConrogateError> {
        let _ = config;
        Ok(())
    }

    async fn before_request(
        &self,
        ctx: &mut PluginContext,
    ) -> Result<PluginOutcome, ConrogateError> {
        // 跳过健康检查路径
        if let Some(http) = &ctx.http {
            if self
                .config
                .skip_paths
                .iter()
                .any(|p| http.path.starts_with(p.as_str()))
            {
                return Ok(PluginOutcome::Continue);
            }

            tracing::info!(
                trace_id = %ctx.trace_id,
                request_id = %ctx.request_id,
                method = %http.method,
                path = %http.path,
                client_ip = %ctx.client_ip,
                "incoming request"
            );
        }

        Ok(PluginOutcome::Continue)
    }

    async fn after_response(
        &self,
        ctx: &mut PluginContext,
        resp: &mut PluginResponse,
    ) -> Result<(), ConrogateError> {
        tracing::info!(
            trace_id = %ctx.trace_id,
            request_id = %ctx.request_id,
            status = resp.status,
            "request completed"
        );
        Ok(())
    }
}
