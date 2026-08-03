//! Conrogate 官方跨域插件：CORS 响应头注入与预检处理。

use async_trait::async_trait;
use conrogate_contract::{
    plugin::{Plugin, PluginContext, PluginKind, PluginOutcome, PluginResponse},
    protocol::ProtocolId,
    ConrogateError,
};
use serde_json::Value;

/// CORS 插件配置
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CorsPluginConfig {
    pub allow_origins: Vec<String>,
    pub allow_methods: Vec<String>,
    pub allow_headers: Vec<String>,
    pub expose_headers: Vec<String>,
    pub allow_credentials: bool,
    pub max_age_seconds: u64,
}

impl Default for CorsPluginConfig {
    fn default() -> Self {
        Self {
            allow_origins: vec!["*".into()],
            allow_methods: vec![
                "GET".into(),
                "POST".into(),
                "PUT".into(),
                "PATCH".into(),
                "DELETE".into(),
                "OPTIONS".into(),
            ],
            allow_headers: vec!["Content-Type".into(), "Authorization".into()],
            expose_headers: vec![],
            allow_credentials: false,
            max_age_seconds: 3600,
        }
    }
}

pub struct CorsPlugin {
    config: CorsPluginConfig,
}

impl CorsPlugin {
    pub fn new() -> Self {
        Self {
            config: CorsPluginConfig::default(),
        }
    }
}

impl Default for CorsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for CorsPlugin {
    fn name(&self) -> &'static str {
        "cors"
    }

    fn kind(&self) -> PluginKind {
        PluginKind::Native
    }

    fn protocols(&self) -> &[ProtocolId] {
        &[ProtocolId::Http]
    }

    fn is_blocking(&self) -> bool {
        false
    }

    fn validate_config(&self, config: &Value) -> Result<(), ConrogateError> {
        if config.is_null() {
            return Ok(());
        }
        serde_json::from_value::<CorsPluginConfig>(config.clone())
            .map(|_| ())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))
    }

    async fn before_request(
        &self,
        ctx: &mut PluginContext,
    ) -> Result<PluginOutcome, ConrogateError> {
        if let Some(http) = &ctx.http {
            // OPTIONS 预检请求直接返回
            if http.method == "OPTIONS" {
                let mut headers = http::HeaderMap::new();
                let origins = self.config.allow_origins.join(", ");
                let methods = self.config.allow_methods.join(", ");
                let allow_headers = self.config.allow_headers.join(", ");

                if let Ok(v) = origins.parse() {
                    headers.insert("Access-Control-Allow-Origin", v);
                }
                if let Ok(v) = methods.parse() {
                    headers.insert("Access-Control-Allow-Methods", v);
                }
                if let Ok(v) = allow_headers.parse() {
                    headers.insert("Access-Control-Allow-Headers", v);
                }
                if let Ok(v) = self.config.max_age_seconds.to_string().parse() {
                    headers.insert("Access-Control-Max-Age", v);
                }
                if self.config.allow_credentials {
                    if let Ok(v) = "true".parse() {
                        headers.insert("Access-Control-Allow-Credentials", v);
                    }
                }

                return Ok(PluginOutcome::Terminate(
                    http::StatusCode::NO_CONTENT,
                    serde_json::Value::Null,
                ));
            }
        }

        Ok(PluginOutcome::Continue)
    }

    async fn after_response(
        &self,
        _ctx: &mut PluginContext,
        resp: &mut PluginResponse,
    ) -> Result<(), ConrogateError> {
        let origins = self.config.allow_origins.join(", ");
        if let Ok(v) = origins.parse() {
            resp.headers.insert("Access-Control-Allow-Origin", v);
        }
        if self.config.allow_credentials {
            if let Ok(v) = "true".parse() {
                resp.headers.insert("Access-Control-Allow-Credentials", v);
            }
        }
        if !self.config.expose_headers.is_empty() {
            let expose = self.config.expose_headers.join(", ");
            if let Ok(v) = expose.parse() {
                resp.headers.insert("Access-Control-Expose-Headers", v);
            }
        }

        Ok(())
    }
}
