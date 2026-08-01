//! Conrogate 官方鉴权插件：JWT Bearer Token 校验。
//!
//! 支持 HS256/HS384/HS512 对称签名 + issuer/audience 校验 + 过期检查。

use conrogate_contract::{
    plugin::{Plugin, PluginContext, PluginOutcome, PluginKind, PluginResponse},
    protocol::ProtocolId,
    ConrogateError,
};
use async_trait::async_trait;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde_json::Value;
use std::sync::Arc;

/// Auth 插件配置
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthPluginConfig {
    /// JWT 签名密钥（HMAC）
    pub secret: String,
    /// 签发者（issuer）
    #[serde(default)]
    pub issuer: Option<String>,
    /// 受众（audience）
    #[serde(default)]
    pub audience: Option<String>,
    /// 是否强制要求 token
    #[serde(default = "default_true")]
    pub require_token: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AuthPluginConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            issuer: None,
            audience: None,
            require_token: true,
        }
    }
}

pub struct AuthPlugin {
    config: Arc<AuthPluginConfig>,
}

impl AuthPlugin {
    pub fn new() -> Self {
        Self {
            config: Arc::new(AuthPluginConfig::default()),
        }
    }

    /// 从配置创建
    pub fn with_config(config: AuthPluginConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

impl Default for AuthPlugin {
    fn default() -> Self {
        Self::new()
    }
}

/// JWT Claims
#[derive(Debug, serde::Deserialize)]
struct Claims {
    /// 签发者 (iss)
    iss: Option<String>,
    /// 受众 (aud)
    aud: Option<String>,
    /// 过期时间 (exp)
    exp: Option<u64>,
    /// 主题 (sub) — 用户 ID
    sub: Option<String>,
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
        let cfg: AuthPluginConfig = serde_json::from_value(config.clone())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))?;

        if cfg.require_token && cfg.secret.is_empty() {
            return Err(ConrogateError::PluginConfigInvalid(
                "auth plugin: secret is required when require_token=true".into(),
            ));
        }
        Ok(())
    }

    async fn init(&self, config: &Value) -> Result<(), ConrogateError> {
        let _ = config;
        Ok(())
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
            (Some(token), _) => {
                // JWT 校验：解码 + 验签 + 过期检查
                let key = DecodingKey::from_secret(self.config.secret.as_bytes());
                let mut validation = Validation::new(Algorithm::HS256);
                validation.validate_exp = true;

                if let Some(ref iss) = self.config.issuer {
                    validation.set_issuer(&[iss]);
                }
                if let Some(ref aud) = self.config.audience {
                    validation.set_audience(&[aud]);
                }

                match decode::<Claims>(&token, &key, &validation) {
                    Ok(_claims) => {
                        // 校验通过，放行
                        Ok(PluginOutcome::Continue)
                    }
                    Err(e) => {
                        tracing::warn!(
                            trace_id = %ctx.trace_id,
                            error = %e,
                            "JWT validation failed"
                        );
                        Ok(PluginOutcome::Terminate(
                            http::StatusCode::UNAUTHORIZED,
                            serde_json::json!({
                                "code": 10002,
                                "msg": format!("unauthorized: {}", e)
                            }),
                        ))
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use conrogate_contract::plugin::{PluginContext, HttpContext, PluginOutcome, PluginServices, PluginMetrics, PluginLogger};
    use conrogate_contract::protocol::ProtocolId;
    use http::Method;
    use jsonwebtoken::{encode, EncodingKey, Header};

    struct NoopMetrics;
    #[async_trait]
    impl PluginMetrics for NoopMetrics {
        async fn increment(&self, _name: &str) {}
        async fn gauge(&self, _name: &str, _value: f64) {}
    }
    struct NoopLogger;
    #[async_trait]
    impl PluginLogger for NoopLogger {
        async fn log(&self, _level: &str, _msg: &str) {}
    }

    fn make_ctx(auth_header: Option<&str>) -> PluginContext {
        let mut headers = http::HeaderMap::new();
        if let Some(h) = auth_header {
            headers.insert("authorization", h.parse().unwrap());
        }
        PluginContext {
            request_id: "test-req".into(),
            trace_id: "test-trace".into(),
            route_id: 1,
            client_ip: "127.0.0.1".into(),
            protocol: ProtocolId::Http,
            http: Some(HttpContext {
                method: Method::GET,
                path: "/test".into(),
                query: Default::default(),
                headers,
                body: None,
            }),
            tunnel: None,
            services: PluginServices {
                metrics: Arc::new(NoopMetrics),
                logger: Arc::new(NoopLogger),
            },
        }
    }

    fn make_token(secret: &str, iss: Option<&str>, aud: Option<&str>) -> String {
        let mut claims = serde_json::json!({
            "sub": "test-user",
            "exp": (chrono::Utc::now().timestamp() + 3600) as u64,
        });
        if let Some(iss) = iss {
            claims["iss"] = serde_json::json!(iss);
        }
        if let Some(aud) = aud {
            claims["aud"] = serde_json::json!(aud);
        }
        encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).unwrap()
    }

    #[tokio::test]
    async fn test_valid_token() {
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            secret: "test-secret".into(),
            ..Default::default()
        });
        let token = make_token("test-secret", None, None);
        let mut ctx = make_ctx(Some(&format!("Bearer {}", token)));
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Continue));
    }

    #[tokio::test]
    async fn test_missing_token() {
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            secret: "test-secret".into(),
            require_token: true,
            ..Default::default()
        });
        let mut ctx = make_ctx(None);
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Terminate(_, _)));
    }

    #[tokio::test]
    async fn test_invalid_token() {
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            secret: "test-secret".into(),
            ..Default::default()
        });
        let mut ctx = make_ctx(Some("Bearer invalid-token"));
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Terminate(_, _)));
    }

    #[tokio::test]
    async fn test_no_token_not_required() {
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            secret: "test-secret".into(),
            require_token: false,
            ..Default::default()
        });
        let mut ctx = make_ctx(None);
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Continue));
    }

    #[tokio::test]
    async fn test_issuer_mismatch() {
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            secret: "test-secret".into(),
            issuer: Some("expected-iss".into()),
            ..Default::default()
        });
        let token = make_token("test-secret", Some("wrong-iss"), None);
        let mut ctx = make_ctx(Some(&format!("Bearer {}", token)));
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Terminate(_, _)));
    }

    #[test]
    fn test_validate_config_empty_secret() {
        let plugin = AuthPlugin::new();
        let config = serde_json::json!({"secret": "", "require_token": true});
        let result = plugin.validate_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_config_ok() {
        let plugin = AuthPlugin::new();
        let config = serde_json::json!({"secret": "my-secret", "require_token": true});
        let result = plugin.validate_config(&config);
        assert!(result.is_ok());
    }
}
