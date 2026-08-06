//! Conrogate 官方跨域插件：CORS 响应头注入与预检处理。

use async_trait::async_trait;
use conrogate_core::contract::{
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

impl CorsPlugin {
    /// 计算 Access-Control-Allow-Origin 取值：
    /// 配置含 `*` 时返回 `*`；否则回显匹配的请求 Origin（CORS 规范禁止多值列表）
    fn resolve_origin(&self, request_origin: Option<&str>) -> Option<String> {
        if self.config.allow_origins.iter().any(|o| o == "*") {
            return Some("*".to_string());
        }
        match request_origin {
            Some(origin) if self.config.allow_origins.iter().any(|o| o == origin) => {
                Some(origin.to_string())
            }
            _ => None,
        }
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
                let request_origin = http.headers.get("origin").and_then(|v| v.to_str().ok());
                if let Some(origin) = self.resolve_origin(request_origin) {
                    if let Ok(v) = origin.parse() {
                        headers.insert("Access-Control-Allow-Origin", v);
                    }
                }
                let methods = self.config.allow_methods.join(", ");
                let allow_headers = self.config.allow_headers.join(", ");

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
        ctx: &mut PluginContext,
        resp: &mut PluginResponse,
    ) -> Result<(), ConrogateError> {
        let request_origin = ctx
            .http
            .as_ref()
            .and_then(|h| h.headers.get("origin"))
            .and_then(|v| v.to_str().ok());
        if let Some(origin) = self.resolve_origin(request_origin) {
            if let Ok(v) = origin.parse() {
                resp.headers.insert("Access-Control-Allow-Origin", v);
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use conrogate_core::contract::plugin::{HttpContext, PluginLogger, PluginMetrics, PluginServices};
    use http::Method;
    use std::sync::Arc;

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

    fn plugin_with_origins(origins: Vec<String>) -> CorsPlugin {
        CorsPlugin {
            config: CorsPluginConfig {
                allow_origins: origins,
                allow_methods: vec!["GET".into(), "OPTIONS".into()],
                allow_headers: vec!["Content-Type".into()],
                expose_headers: vec![],
                allow_credentials: false,
                max_age_seconds: 3600,
            },
        }
    }

    fn ctx_with_origin(origin: Option<&str>) -> PluginContext {
        let mut headers = http::HeaderMap::new();
        if let Some(o) = origin {
            headers.insert("origin", o.parse().unwrap());
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

    /// 多 origin 配置：匹配的 Origin 被回显，而非拼接多值列表
    #[tokio::test]
    async fn multi_origin_echoes_matching_origin() {
        let plugin = plugin_with_origins(vec![
            "https://a.example.com".into(),
            "https://b.example.com".into(),
        ]);
        let mut ctx = ctx_with_origin(Some("https://b.example.com"));
        let mut resp = PluginResponse {
            status: 200,
            headers: http::HeaderMap::new(),
            body: bytes::Bytes::new(),
        };

        let plugin: &dyn Plugin = &plugin;
        let result = plugin.after_response(&mut ctx, &mut resp).await;
        assert!(result.is_ok());
        assert_eq!(
            resp.headers.get("access-control-allow-origin").unwrap(),
            "https://b.example.com"
        );
    }

    /// 未匹配的 Origin：不注入 Allow-Origin 头
    #[tokio::test]
    async fn unmatched_origin_omits_header() {
        let plugin = plugin_with_origins(vec!["https://a.example.com".into()]);
        let mut ctx = ctx_with_origin(Some("https://evil.example.com"));
        let mut resp = PluginResponse {
            status: 200,
            headers: http::HeaderMap::new(),
            body: bytes::Bytes::new(),
        };

        let plugin: &dyn Plugin = &plugin;
        let result = plugin.after_response(&mut ctx, &mut resp).await;
        assert!(result.is_ok());
        assert!(resp.headers.get("access-control-allow-origin").is_none());
    }

    /// 通配配置：任意请求返回 *
    #[tokio::test]
    async fn wildcard_origin_returns_star() {
        let plugin = plugin_with_origins(vec!["*".into()]);
        let mut ctx = ctx_with_origin(Some("https://any.example.com"));
        let mut resp = PluginResponse {
            status: 200,
            headers: http::HeaderMap::new(),
            body: bytes::Bytes::new(),
        };

        let plugin: &dyn Plugin = &plugin;
        let result = plugin.after_response(&mut ctx, &mut resp).await;
        assert!(result.is_ok());
        assert_eq!(
            resp.headers.get("access-control-allow-origin").unwrap(),
            "*"
        );
    }
}
