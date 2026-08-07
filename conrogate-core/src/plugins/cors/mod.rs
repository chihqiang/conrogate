//! 
//! CORS 跨域插件（Conrogate 官方内置插件）。
//! 
//! - 插件名：`cors`
//! - 协议：HTTP
//! - 阻断性：`blocking = false`
//! - 是否需要请求体：否
//! 
//! ## 原理
//! 
//! 在网关层统一处理浏览器跨域，避免上游各自配置。核心逻辑：
//! 
//! - **OPTIONS 预检请求**：在 `before_request` 阶段直接拦截，返回 `204 No Content` 并注入 CORS 响应头，**不再转发给上游**。
//! - **正常请求**：在 `after_response` 阶段向真实响应注入 `Access-Control-Allow-Origin` 等头，透传上游结果。
//! 
//! Origin 匹配策略（`resolve_origin`）：
//! 
//! - 配置含 `*` → 直接返回 `Access-Control-Allow-Origin: *`。
//! - 否则按白名单**精确匹配**请求 `Origin`，命中后**回显该 Origin**（CORS 规范禁止返回多值列表，所以不会拼接多个域名）。
//! - 未命中 → 不注入 CORS 头（浏览器会拦截响应）。
//! 
//! 每个绑定拥有独立配置实例，不同路由可配置不同的跨域策略。
//! 
//! ## 请求过程
//! 
//! ```text
//! 客户端（浏览器）→ 网关
//!  ┌─ 预检请求 OPTIONS
//!  │    cors.before_request
//!  │      ├─ 命中白名单 Origin → 返回 204 + CORS 头（不转发上游）
//!  │      └─ 未命中            → 直接终止（无 CORS 头）
//!  │
//!  └─ 正常请求（GET/POST/...）
//!       cors.before_request → 放行（非 OPTIONS）
//!       转发到上游
//!       cors.after_response → 注入 Access-Control-Allow-Origin 等头
//!       返回客户端
//! ```
//! 
//! ## 配置
//! 
//! 绑定到路由时，`config` 支持以下字段：
//! 
//! | 字段 | 类型 | 默认 | 说明 |
//! | --- | --- | --- | --- |
//! | `allow_origins` | string[] | `["*"]` | 允许的 Origin 白名单 |
//! | `allow_methods` | string[] | GET/POST/PUT/PATCH/DELETE/OPTIONS | 预检响应 `Access-Control-Allow-Methods` |
//! | `allow_headers` | string[] | `["Content-Type","Authorization"]` | 预检响应 `Access-Control-Allow-Headers` |
//! | `expose_headers` | string[] | `[]` | 响应 `Access-Control-Expose-Headers`（允许前端读取的响应头） |
//! | `allow_credentials` | bool | `false` | 是否允许携带 Cookie（`Access-Control-Allow-Credentials: true`） |
//! | `max_age_seconds` | int | `3600` | 预检缓存时长 `Access-Control-Max-Age` |
//! 
//! > 注意：`allow_credentials=true` 时浏览器要求 `Access-Control-Allow-Origin` 不能是 `*`，应配置为具体域名。
//! 
//! ## 使用
//! 
//! ### 1. 绑定插件到路由
//! 
//! ```bash
//! curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
//!   -H 'Content-Type: application/json' \
//!   -d '{
//!     "plugin_name": "cors",
//!     "config": {
//!       "allow_origins": ["https://app.example.com", "https://admin.example.com"],
//!       "allow_methods": ["GET","POST","OPTIONS"],
//!       "allow_headers": ["Content-Type","Authorization","X-Custom"],
//!       "expose_headers": ["X-Trace-Id","X-Request-Id"],
//!       "allow_credentials": true,
//!       "max_age_seconds": 600
//!     },
//!     "order": 0,
//!     "blocking": false,
//!     "enabled": true
//!   }'
//! ```
//! 
//! ### 2. 发布配置
//! 
//! ```bash
//! curl -X POST http://<控制面>:9000/api/v1/configs/publish
//! ```
//! 
//! ### 3. 验证
//! 
//! ```bash
//! # 预检请求 → 204 + CORS 头（不触达上游）
//! curl -i -X OPTIONS http://<网关>:8080/your/path \
//!   -H "Origin: https://app.example.com" \
//!   -H "Access-Control-Request-Method: POST"
//! 
//! # 正常请求 → 200 + Access-Control-Allow-Origin 回显
//! curl -i http://<网关>:8080/your/path -H "Origin: https://app.example.com"
//! ```
//! 
//! ## 注意事项
//! 
//! - 未命中白名单的 Origin：预检返回 204 但**不带** CORS 头（浏览器拒绝跨域读取），正常请求响应也不注入 CORS 头。
//! - 插件不读取请求体，不影响流式转发路径。
//!
//!
//! Conrogate 官方跨域插件：CORS 响应头注入与预检处理。

use async_trait::async_trait;
use crate::contract::{
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

    fn configured(&self, config: &Value) -> Result<std::sync::Arc<dyn Plugin>, ConrogateError> {
        if config.is_null() {
            return Ok(std::sync::Arc::new(CorsPlugin::new()));
        }
        let cfg: CorsPluginConfig = serde_json::from_value(config.clone())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))?;
        Ok(std::sync::Arc::new(CorsPlugin { config: cfg }))
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
    use crate::contract::plugin::{HttpContext, PluginLogger, PluginMetrics, PluginServices};
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
