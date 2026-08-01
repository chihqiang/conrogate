//! Conrogate 官方鉴权插件：JWT Bearer Token 校验。

use conrogate_contract::{
    plugin::{Plugin, PluginContext, PluginOutcome, PluginKind},
    protocol::ProtocolId,
    ConrogateError,
};
use async_trait::async_trait;
use serde_json::Value;

/// Auth 插件配置
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthPluginConfig {
    pub issuer: String,
    pub audience: String,
    pub jwks_url: String,
    pub cache_ttl_seconds: u64,
    pub require_token: bool,
}

impl Default for AuthPluginConfig {
    fn default() -> Self {
        Self {
            issuer: String::new(),
            audience: String::new(),
            jwks_url: String::new(),
            cache_ttl_seconds: 300,
            require_token: true,
        }
    }
}

pub struct AuthPlugin {
    config: AuthPluginConfig,
}

impl AuthPlugin {
    pub fn new() -> Self {
        Self {
            config: AuthPluginConfig::default(),
        }
    }
}

impl Default for AuthPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for AuthPlugin {
    fn name(&self) -> &'static str {
        "auth"
    }

    fn kind(&self) -> PluginKind {
        PluginKind::Native
    }

    fn protocols(&self) -> &[ProtocolId] {
        &[ProtocolId::Http, ProtocolId::WebSocket]
    }

    fn is_blocking(&self) -> bool {
        true
    }

    fn validate_config(&self, config: &Value) -> Result<(), ConrogateError> {
        if config.is_null() {
            return Ok(());
        }
        serde_json::from_value::<AuthPluginConfig>(config.clone())
            .map(|_| ())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))
    }

    async fn before_request(
        &self,
        ctx: &mut PluginContext,
    ) -> Result<PluginOutcome, ConrogateError> {
        let require_token = self.config.require_token;

        // 从 HTTP 头提取 Authorization
        let token = ctx
            .http
            .as_ref()
            .and_then(|h| h.headers.get("authorization"))
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string());

        match (token, require_token) {
            (Some(_token), _) => {
                // TODO: JWT 校验逻辑（解码 + 验签 + 过期检查 + issuer/audience 匹配）
                // 当前骨架仅检查 token 存在性
                Ok(PluginOutcome::Continue)
            }
            (None, true) => {
                // 需要鉴权但无 token
                Ok(PluginOutcome::Terminate(
                    http::StatusCode::UNAUTHORIZED,
                    serde_json::json!({
                        "code": 10002,
                        "msg": "unauthorized: missing bearer token"
                    }),
                ))
            }
            (None, false) => Ok(PluginOutcome::Continue),
        }
    }
}
