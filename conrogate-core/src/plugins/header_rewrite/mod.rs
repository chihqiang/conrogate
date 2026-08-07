//!
//! 请求 / 响应头改写插件（Conrogate 官方内置插件）。
//!
//! - 插件名：`header_rewrite`
//! - 协议：HTTP
//! - 阻断性：`blocking = false`
//! - 是否需要请求体：否
//!
//! ## 原理
//!
//! 按路由绑定级配置，在请求转发前（`before_request`）改写请求头、
//! 在响应回包前（`after_response`）改写响应头，全程不拦截、不读取请求体。
//!
//! 配置分 `request` / `response` 两段，每段支持三类操作：
//!
//! | 操作 | 语义 |
//! | --- | --- |
//! | `set` | 覆盖同名头的所有值；头不存在则新增 |
//! | `add` | 追加一个值，不覆盖已有值（同名字头可共存） |
//! | `remove` | 删除该头（存在即移除） |
//!
//! `set` / `add` 的值支持以下占位符，运行时替换为真实上下文：
//!
//! | 占位符 | 含义 |
//! | --- | --- |
//! | `$client_ip` | 客户端 IP |
//! | `$request_id` | 请求 ID |
//! | `$trace_id` | 链路 trace ID |
//! | `$route_id` | 命中的路由 ID |
//! | `$method` | 请求方法（仅请求段有效，响应段为空串） |
//! | `$path` | 请求路径（仅请求段有效，响应段为空串） |
//!
//! ## 配置
//!
//! 绑定到路由时，`config` 支持以下字段（两段均可选，缺省为空）：
//!
//! | 字段 | 类型 | 默认 | 说明 |
//! | --- | --- | --- | --- |
//! | `request.set` | object | `{}` | 覆盖请求头，键为头名，值为新值 |
//! | `request.add` | object | `{}` | 追加请求头值 |
//! | `request.remove` | string[] | `[]` | 删除的请求头名列表 |
//! | `response.set` | object | `{}` | 覆盖响应头 |
//! | `response.add` | object | `{}` | 追加响应头值 |
//! | `response.remove` | string[] | `[]` | 删除的响应头名列表 |
//!
//! > 头名必须为合法的 HTTP 头名；值不能包含 CR / LF 等控制字符（防响应头注入）。
//!
//! ## 使用
//!
//! ### 1. 绑定插件到路由
//!
//! ```bash
//! curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
//!   -H 'Content-Type: application/json' \
//!   -d '{
//!     "plugin_name": "header_rewrite",
//!     "config": {
//!       "request": {
//!         "set": { "X-Real-IP": "$client_ip", "X-Gateway": "conrogate" },
//!         "add": { "X-Custom": "value" },
//!         "remove": ["X-Internal-Token"]
//!       },
//!       "response": {
//!         "set": { "X-Powered-By": "conrogate" },
//!         "remove": ["X-Debug"]
//!       }
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
//! curl -i http://<网关>:8080/your/path
//! # 响应头出现 X-Powered-By: conrogate，且不再有 X-Debug
//! ```
//!
//! ## 注意事项
//!
//! - 插件不读取请求体，不影响流式转发路径。
//! - `set` / `add` 顺序对结果无影响；`remove` 优先于 `set` / `add` 执行。
//! - 未识别的占位符按原样透传（不做替换）。
//!
//!
//! Conrogate 官方请求 / 响应头改写插件。

use crate::contract::{
    plugin::{Plugin, PluginContext, PluginKind, PluginOutcome, PluginResponse},
    protocol::ProtocolId,
    ConrogateError,
};
use async_trait::async_trait;
use http::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// 单方向（请求 / 响应）的改写规则
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct HeaderRewriteSection {
    /// 覆盖同名字头的所有值；不存在则新增
    pub set: HashMap<String, String>,
    /// 追加一个值，不覆盖已有值
    pub add: HashMap<String, String>,
    /// 删除的头名列表
    pub remove: Vec<String>,
}

/// header_rewrite 插件配置
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct HeaderRewriteConfig {
    #[serde(default)]
    pub request: HeaderRewriteSection,
    #[serde(default)]
    pub response: HeaderRewriteSection,
}

pub struct HeaderRewritePlugin {
    config: HeaderRewriteConfig,
}

/// 占位符替换所需的请求上下文快照（避免与可变借用 `ctx.http.headers` 冲突）
#[derive(Debug, Clone)]
struct PlaceholderCtx {
    client_ip: String,
    request_id: String,
    trace_id: String,
    route_id: u64,
    method: String,
    path: String,
}

impl PlaceholderCtx {
    fn from(ctx: &PluginContext) -> Self {
        let (method, path) = match &ctx.http {
            Some(h) => (h.method.as_str().to_string(), h.path.clone()),
            None => (String::new(), String::new()),
        };
        Self {
            client_ip: ctx.client_ip.clone(),
            request_id: ctx.request_id.clone(),
            trace_id: ctx.trace_id.clone(),
            route_id: ctx.route_id,
            method,
            path,
        }
    }
}

impl HeaderRewritePlugin {
    pub fn new() -> Self {
        Self {
            config: HeaderRewriteConfig::default(),
        }
    }
}

impl Default for HeaderRewritePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl HeaderRewritePlugin {
    /// 反序列化并校验配置（validate_config 与 configured 共用）
    fn parse_config(config: &Value) -> Result<HeaderRewriteConfig, ConrogateError> {
        let cfg: HeaderRewriteConfig = serde_json::from_value(config.clone())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))?;
        Self::validate_section(&cfg.request)?;
        Self::validate_section(&cfg.response)?;
        Ok(cfg)
    }

    /// 校验单段规则：头名合法 + 值不含控制字符（防响应头注入）
    fn validate_section(section: &HeaderRewriteSection) -> Result<(), ConrogateError> {
        for name in section
            .set
            .keys()
            .chain(section.add.keys())
            .chain(section.remove.iter())
        {
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                ConrogateError::PluginConfigInvalid(format!("invalid header name: {name}"))
            })?;
        }
        for value in section.set.values().chain(section.add.values()) {
            HeaderValue::from_str(value).map_err(|_| {
                ConrogateError::PluginConfigInvalid(format!(
                    "invalid header value for {:?}: contains CR/LF or control characters",
                    value
                ))
            })?;
        }
        Ok(())
    }

    /// 占位符替换：`$client_ip` / `$request_id` / `$trace_id` / `$route_id` / `$method` / `$path`
    fn resolve_value(raw: &str, ph: &PlaceholderCtx) -> String {
        raw.replace("$client_ip", &ph.client_ip)
            .replace("$request_id", &ph.request_id)
            .replace("$trace_id", &ph.trace_id)
            .replace("$route_id", &ph.route_id.to_string())
            .replace("$method", &ph.method)
            .replace("$path", &ph.path)
    }

    /// 应用一段规则到头集合
    fn apply_section(section: &HeaderRewriteSection, headers: &mut HeaderMap, ph: &PlaceholderCtx) {
        for name in &section.remove {
            if let Ok(n) = HeaderName::from_bytes(name.as_bytes()) {
                headers.remove(n);
            }
        }
        for (name, raw) in &section.set {
            let Ok(n) = HeaderName::from_bytes(name.as_bytes()) else {
                continue;
            };
            let Ok(v) = HeaderValue::from_str(&Self::resolve_value(raw, ph)) else {
                continue;
            };
            headers.insert(n, v);
        }
        for (name, raw) in &section.add {
            let Ok(n) = HeaderName::from_bytes(name.as_bytes()) else {
                continue;
            };
            let Ok(v) = HeaderValue::from_str(&Self::resolve_value(raw, ph)) else {
                continue;
            };
            headers.append(n, v);
        }
    }
}

#[async_trait]
impl Plugin for HeaderRewritePlugin {
    fn name(&self) -> &'static str {
        "header_rewrite"
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
        Self::parse_config(config).map(|_| ())
    }

    fn configured(&self, config: &Value) -> Result<Arc<dyn Plugin>, ConrogateError> {
        if config.is_null() {
            return Ok(Arc::new(HeaderRewritePlugin::new()));
        }
        let cfg = Self::parse_config(config)?;
        Ok(Arc::new(HeaderRewritePlugin { config: cfg }))
    }

    async fn before_request(
        &self,
        ctx: &mut PluginContext,
    ) -> Result<PluginOutcome, ConrogateError> {
        let ph = PlaceholderCtx::from(ctx);
        if let Some(http) = &mut ctx.http {
            Self::apply_section(&self.config.request, &mut http.headers, &ph);
        }
        Ok(PluginOutcome::Continue)
    }

    async fn after_response(
        &self,
        ctx: &mut PluginContext,
        resp: &mut PluginResponse,
    ) -> Result<(), ConrogateError> {
        let ph = PlaceholderCtx::from(ctx);
        Self::apply_section(&self.config.response, &mut resp.headers, &ph);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::plugin::{HttpContext, PluginLogger, PluginMetrics, PluginServices};
    use http::Method;

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

    fn base_ctx() -> PluginContext {
        PluginContext {
            request_id: "req-1".into(),
            trace_id: "trace-1".into(),
            route_id: 7,
            client_ip: "10.0.0.1".into(),
            protocol: ProtocolId::Http,
            http: Some(HttpContext {
                method: Method::GET,
                path: "/api/users".into(),
                query: Default::default(),
                headers: HeaderMap::new(),
                body: None,
            }),
            tunnel: None,
            services: PluginServices {
                metrics: Arc::new(NoopMetrics),
                logger: Arc::new(NoopLogger),
            },
        }
    }

    fn config(request: Value, response: Value) -> HeaderRewriteConfig {
        serde_json::from_value(serde_json::json!({
            "request": request,
            "response": response,
        }))
        .unwrap()
    }

    /// 请求段 set：覆盖已有头 + 新增头 + 占位符替换
    #[tokio::test]
    async fn request_set_overwrites_and_resolves_placeholders() {
        let mut ctx = base_ctx();
        ctx.http
            .as_mut()
            .unwrap()
            .headers
            .insert("x-existing", HeaderValue::from_static("old"));
        let plugin = HeaderRewritePlugin {
            config: config(
                serde_json::json!({
                    "set": {
                        "X-Existing": "new",
                        "X-Real-IP": "$client_ip",
                        "X-Trace": "$trace_id",
                    }
                }),
                serde_json::json!({}),
            ),
        };

        let plugin: &dyn Plugin = &plugin;
        let outcome = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(outcome, PluginOutcome::Continue));

        let headers = &ctx.http.as_ref().unwrap().headers;
        assert_eq!(headers.get("x-existing").unwrap(), "new");
        assert_eq!(headers.get("x-real-ip").unwrap(), "10.0.0.1");
        assert_eq!(headers.get("x-trace").unwrap(), "trace-1");
    }

    /// 请求段 add：追加不覆盖已有值
    #[tokio::test]
    async fn request_add_appends_without_overwrite() {
        let mut ctx = base_ctx();
        ctx.http
            .as_mut()
            .unwrap()
            .headers
            .append("x-multi", HeaderValue::from_static("a"));
        let plugin = HeaderRewritePlugin {
            config: config(
                serde_json::json!({ "add": { "X-Multi": "b" } }),
                serde_json::json!({}),
            ),
        };

        let plugin: &dyn Plugin = &plugin;
        let _ = plugin.before_request(&mut ctx).await.unwrap();

        let headers = &ctx.http.as_mut().unwrap().headers;
        let values: Vec<_> = headers
            .get_all("x-multi")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(values, vec!["a", "b"]);
    }

    /// 请求段 remove：删除指定头
    #[tokio::test]
    async fn request_remove_deletes_header() {
        let mut ctx = base_ctx();
        ctx.http
            .as_mut()
            .unwrap()
            .headers
            .insert("x-internal", HeaderValue::from_static("secret"));
        let plugin = HeaderRewritePlugin {
            config: config(
                serde_json::json!({ "remove": ["X-Internal"] }),
                serde_json::json!({}),
            ),
        };

        let plugin: &dyn Plugin = &plugin;
        let _ = plugin.before_request(&mut ctx).await.unwrap();

        assert!(ctx
            .http
            .as_ref()
            .unwrap()
            .headers
            .get("x-internal")
            .is_none());
    }

    /// 响应段：set / remove 作用于响应头
    #[tokio::test]
    async fn response_section_modifies_response_headers() {
        let mut ctx = base_ctx();
        let mut resp = PluginResponse {
            status: 200,
            headers: HeaderMap::new(),
            body: bytes::Bytes::new(),
        };
        resp.headers
            .insert("x-debug", HeaderValue::from_static("on"));
        let plugin = HeaderRewritePlugin {
            config: config(
                serde_json::json!({}),
                serde_json::json!({
                    "set": { "X-Powered-By": "conrogate" },
                    "remove": ["X-Debug"],
                }),
            ),
        };

        let plugin: &dyn Plugin = &plugin;
        plugin.after_response(&mut ctx, &mut resp).await.unwrap();

        assert_eq!(resp.headers.get("x-powered-by").unwrap(), "conrogate");
        assert!(resp.headers.get("x-debug").is_none());
    }

    /// validate_config：拒绝含 CR/LF 的头值（防注入）
    #[tokio::test]
    async fn validate_rejects_header_injection() {
        let plugin = HeaderRewritePlugin::new();
        let bad = serde_json::json!({
            "response": { "set": { "X-Evil": "ok\r\nSet-Cookie: evil=1" } }
        });
        let err = plugin.validate_config(&bad);
        assert!(matches!(err, Err(ConrogateError::PluginConfigInvalid(_))));
    }

    /// validate_config：拒绝非法头名
    #[tokio::test]
    async fn validate_rejects_invalid_header_name() {
        let plugin = HeaderRewritePlugin::new();
        let bad = serde_json::json!({ "request": { "remove": ["bad name"] } });
        let err = plugin.validate_config(&bad);
        assert!(matches!(err, Err(ConrogateError::PluginConfigInvalid(_))));
    }

    /// validate_config：合法配置通过，空配置（null）通过
    #[tokio::test]
    async fn validate_accepts_well_formed_config() {
        let plugin = HeaderRewritePlugin::new();
        assert!(plugin.validate_config(&Value::Null).is_ok());
        let ok = serde_json::json!({
            "request": { "set": { "X-A": "1" }, "remove": ["X-B"] },
            "response": { "add": { "X-C": "2" } }
        });
        assert!(plugin.validate_config(&ok).is_ok());
    }

    /// configured：非法配置返回错误
    #[tokio::test]
    async fn configured_rejects_invalid_config() {
        let plugin = HeaderRewritePlugin::new();
        let bad = serde_json::json!({ "response": { "set": { "X-Evil": "a\r\nb" } } });
        assert!(plugin.configured(&bad).is_err());
    }
}
