//!
//! IP 访问控制插件（Conrogate 官方内置插件）。
//!
//! - 插件名：`ip_allow_deny`
//! - 协议：HTTP、WebSocket（升级握手阶段）、TCP 隧道
//! - 阻断性：`blocking = true`（拒绝时直接终止，返回 403）
//! - 是否需要请求体：否
//!
//! ## 原理
//!
//! 按路由绑定级配置，对客户端 IP（`ctx.client_ip`，已按可信代理链路解析出的真实 IP）
//! 做 allow / deny 访问控制：
//!
//! | 配置 | 语义 |
//! | --- | --- |
//! | `deny` 非空且命中 | 一律拒绝（**deny 优先**，即使同时命中 allow） |
//! | `allow` 非空且未命中 | 拒绝 |
//! | `allow` / `deny` 均为空 | 配置非法，绑定直接被拒 |
//! | `allow` 为空（无白名单） | 仅按 deny 拦截 |
//!
//! 拒绝时返回 HTTP 403，响应体为 `{"code":10003,"msg":"forbidden: ip not allowed"}`
//! （与全局 IP 黑名单保持一致）。TCP 隧道在连接建立阶段（`on_connect`）即被拒绝。
//!
//! ## 配置
//!
//! 绑定到路由时，`config` 支持以下字段（均为 string[]，元素为 IP 或 CIDR 网段，
//! 支持 IPv4 / IPv6，裸 IP 视作 /32 或 /128）：
//!
//! | 字段 | 类型 | 默认 | 说明 |
//! | --- | --- | --- | --- |
//! | `allow` | string[] | `[]` | 仅允许的 IP / 网段列表；为空表示不启用白名单 |
//! | `deny` | string[] | `[]` | 拒绝的 IP / 网段列表；为空表示不启用黑名单 |
//!
//! > 配置校验在绑定 API 即时执行：任一条目解析失败、或 allow/deny 同时为空都会拒绝绑定。
//!
//! ## 使用
//!
//! ### 1. 绑定插件到路由
//!
//! ```bash
//! curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
//!   -H 'Content-Type: application/json' \
//!   -d '{
//!     "plugin_name": "ip_allow_deny",
//!     "config": {
//!       "allow": ["10.0.0.0/8", "192.168.1.0/24"],
//!       "deny": ["10.20.0.0/16"]
//!     },
//!     "order": 0,
//!     "blocking": true,
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
//! # 来自 10.20.0.5（命中 deny）→ 403
//! curl -i -H "X-Forwarded-For: 10.20.0.5" http://<网关>:8080/your/path
//!
//! # 来自 10.1.0.5（allow 内、deny 外）→ 放行
//! curl -i -H "X-Forwarded-For: 10.1.0.5" http://<网关>:8080/your/path
//! ```
//!
//! ## 注意事项
//!
//! - 该插件与全局 IP 黑名单互相独立：全局黑名单在任何路由前先拦截，命中即 403；
//!   本插件提供**绑定级**（按路由）更细粒度的 allow/deny 控制。
//! - 插件不读取请求体，路由保持流式转发路径。
//!
//!
//! Conrogate 官方 IP 访问控制插件。
//!
//! 支持：
//! - allow 白名单（非空时仅放行列表内 IP）
//! - deny 黑名单（deny 优先）
//! - HTTP / WebSocket / TCP 隧道三协议
//! - IPv4 / IPv6 / CIDR

use crate::contract::{
    plugin::{Plugin, PluginContext, PluginKind, PluginOutcome},
    protocol::ProtocolId,
    response, ConrogateError,
};
use crate::security::blacklist::parse_ip_or_cidr;
use async_trait::async_trait;
use http::StatusCode;
use ipnet::IpNet;
use serde_json::Value;
use std::sync::Arc;

/// ip_allow_deny 插件配置
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct IpAllowDenyConfig {
    /// 仅允许的 IP / 网段列表（为空 = 不启用白名单）
    #[serde(default)]
    pub allow: Vec<String>,
    /// 拒绝的 IP / 网段列表（为空 = 不启用黑名单；命中时优先拒绝）
    #[serde(default)]
    pub deny: Vec<String>,
}

/// 编译期预解析的规则快照（避免请求路径重复解析字符串）
#[derive(Debug, Clone, Default)]
struct Compiled {
    allow: Vec<IpNet>,
    deny: Vec<IpNet>,
}

pub struct IpAllowDenyPlugin {
    compiled: Compiled,
}

impl IpAllowDenyPlugin {
    pub fn new() -> Self {
        Self {
            compiled: Compiled::default(),
        }
    }

    /// 校验配置：所有条目必须为合法 IP/CIDR，allow/deny 不可同时为空
    fn validate_cfg(cfg: &IpAllowDenyConfig) -> Result<(), ConrogateError> {
        if cfg.allow.is_empty() && cfg.deny.is_empty() {
            return Err(ConrogateError::PluginConfigInvalid(
                "ip_allow_deny: allow and deny cannot both be empty".into(),
            ));
        }
        for entry in cfg.allow.iter().chain(cfg.deny.iter()) {
            if parse_ip_or_cidr(entry).is_none() {
                return Err(ConrogateError::PluginConfigInvalid(format!(
                    "ip_allow_deny: invalid ip or cidr: {entry}"
                )));
            }
        }
        Ok(())
    }

    fn compile(cfg: &IpAllowDenyConfig) -> Compiled {
        Compiled {
            allow: cfg
                .allow
                .iter()
                .filter_map(|s| parse_ip_or_cidr(s))
                .collect(),
            deny: cfg
                .deny
                .iter()
                .filter_map(|s| parse_ip_or_cidr(s))
                .collect(),
        }
    }

    /// 判定结果：None = 放行；Some(403 响应体) = 拒绝
    fn decide(&self, client_ip: &str, trace_id: &str) -> Option<Value> {
        let ip = parse_ip_or_cidr(client_ip)?;
        let denied = self.compiled.deny.iter().any(|net| net.contains(&ip))
            || (!self.compiled.allow.is_empty()
                && !self.compiled.allow.iter().any(|net| net.contains(&ip)));
        if denied {
            return Some(response::error_body_with_trace(
                trace_id,
                ConrogateError::ERR_FORBIDDEN,
                "forbidden: ip not allowed",
            ));
        }
        None
    }
}

impl Default for IpAllowDenyPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for IpAllowDenyPlugin {
    fn name(&self) -> &'static str {
        "ip_allow_deny"
    }

    fn kind(&self) -> PluginKind {
        PluginKind::Native
    }

    fn protocols(&self) -> &[ProtocolId] {
        &[
            ProtocolId::Http,
            ProtocolId::WebSocket,
            ProtocolId::TcpTunnel,
        ]
    }

    fn is_blocking(&self) -> bool {
        true
    }

    fn validate_config(&self, config: &Value) -> Result<(), ConrogateError> {
        if config.is_null() {
            return Ok(());
        }
        let cfg: IpAllowDenyConfig = serde_json::from_value(config.clone())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))?;
        Self::validate_cfg(&cfg)
    }

    fn configured(&self, config: &Value) -> Result<Arc<dyn Plugin>, ConrogateError> {
        if config.is_null() {
            return Ok(Arc::new(IpAllowDenyPlugin::new()));
        }
        let cfg: IpAllowDenyConfig = serde_json::from_value(config.clone())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))?;
        Self::validate_cfg(&cfg)?;
        Ok(Arc::new(IpAllowDenyPlugin {
            compiled: Self::compile(&cfg),
        }))
    }

    async fn before_request(
        &self,
        ctx: &mut PluginContext,
    ) -> Result<PluginOutcome, ConrogateError> {
        if let Some(body) = self.decide(&ctx.client_ip, &ctx.trace_id) {
            return Ok(PluginOutcome::Terminate(StatusCode::FORBIDDEN, body));
        }
        Ok(PluginOutcome::Continue)
    }

    async fn on_connect(&self, ctx: &mut PluginContext) -> Result<PluginOutcome, ConrogateError> {
        if let Some(body) = self.decide(&ctx.client_ip, &ctx.trace_id) {
            return Ok(PluginOutcome::Terminate(StatusCode::FORBIDDEN, body));
        }
        Ok(PluginOutcome::Continue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::plugin::{HttpContext, PluginLogger, PluginMetrics, PluginServices};
    use http::Method;
    use serde_json::json;

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

    fn ctx_with_ip(ip: &str) -> PluginContext {
        PluginContext {
            request_id: "req-1".into(),
            trace_id: "trace-1".into(),
            route_id: 7,
            client_ip: ip.into(),
            protocol: ProtocolId::Http,
            http: Some(HttpContext {
                method: Method::GET,
                path: "/".into(),
                query: Default::default(),
                headers: http::HeaderMap::new(),
                body: None,
            }),
            tunnel: None,
            services: PluginServices {
                metrics: Arc::new(NoopMetrics),
                logger: Arc::new(NoopLogger),
            },
        }
    }

    fn plugin(allow: Value, deny: Value) -> IpAllowDenyPlugin {
        let cfg: IpAllowDenyConfig =
            serde_json::from_value(json!({ "allow": allow, "deny": deny })).unwrap();
        IpAllowDenyPlugin {
            compiled: IpAllowDenyPlugin::compile(&cfg),
        }
    }

    #[tokio::test]
    async fn deny_hits_ipv4() {
        let p = plugin(json!(["10.0.0.0/8"]), json!(["10.20.0.5"]));
        let mut ctx = ctx_with_ip("10.20.0.5");
        let out = p.before_request(&mut ctx).await.unwrap();
        assert!(
            matches!(out, PluginOutcome::Terminate(s, b) if s == StatusCode::FORBIDDEN && b["code"] == ConrogateError::ERR_FORBIDDEN)
        );
    }

    #[tokio::test]
    async fn deny_takes_priority_over_allow() {
        let p = plugin(json!(["10.0.0.0/8"]), json!(["10.1.1.1"]));
        let mut ctx = ctx_with_ip("10.1.1.1");
        let out = p.before_request(&mut ctx).await.unwrap();
        assert!(matches!(out, PluginOutcome::Terminate(_, _)));
    }

    #[tokio::test]
    async fn allow_whitelist_rejects_outside() {
        let p = plugin(json!(["10.0.0.0/8"]), json!([]));
        let mut ctx = ctx_with_ip("192.168.1.5");
        let out = p.before_request(&mut ctx).await.unwrap();
        assert!(matches!(out, PluginOutcome::Terminate(_, _)));
    }

    #[tokio::test]
    async fn allow_whitelist_passes_inside() {
        let p = plugin(json!(["10.0.0.0/8"]), json!([]));
        let mut ctx = ctx_with_ip("10.2.3.4");
        let out = p.before_request(&mut ctx).await.unwrap();
        assert!(matches!(out, PluginOutcome::Continue));
    }

    #[tokio::test]
    async fn empty_allow_deny_allows_everything() {
        let p = plugin(json!([]), json!([]));
        // 直接构造（绕过校验）验证放行语义
        let mut ctx = ctx_with_ip("8.8.8.8");
        let out = p.before_request(&mut ctx).await.unwrap();
        assert!(matches!(out, PluginOutcome::Continue));
    }

    #[tokio::test]
    async fn bare_ip_and_ipv6_cidr() {
        let p = plugin(json!(["1.2.3.4", "2001:db8::/32"]), json!(["5.6.7.8"]));
        let mut ctx = ctx_with_ip("1.2.3.4");
        assert!(matches!(
            p.before_request(&mut ctx).await.unwrap(),
            PluginOutcome::Continue
        ));
        let mut ctx = ctx_with_ip("2001:db8::1");
        assert!(matches!(
            p.before_request(&mut ctx).await.unwrap(),
            PluginOutcome::Continue
        ));
        let mut ctx = ctx_with_ip("5.6.7.8");
        assert!(matches!(
            p.before_request(&mut ctx).await.unwrap(),
            PluginOutcome::Terminate(_, _)
        ));
        let mut ctx = ctx_with_ip("2001:db9::1");
        assert!(matches!(
            p.before_request(&mut ctx).await.unwrap(),
            PluginOutcome::Terminate(_, _)
        ));
    }

    #[tokio::test]
    async fn on_connect_rejects_denied_tunnel() {
        let p = plugin(json!(["10.0.0.0/8"]), json!(["10.99.0.0/16"]));
        let mut ctx = ctx_with_ip("10.99.1.2");
        ctx.protocol = ProtocolId::TcpTunnel;
        ctx.tunnel = Some(crate::contract::plugin::TunnelContext {
            remote_addr: "10.99.1.2:1234".into(),
            sni: None,
            alpn: None,
            listen_port: 8080,
        });
        let out = p.on_connect(&mut ctx).await.unwrap();
        assert!(matches!(out, PluginOutcome::Terminate(_, _)));
    }

    #[test]
    fn validate_rejects_empty_lists() {
        let p = IpAllowDenyPlugin::new();
        let err = p.validate_config(&json!({ "allow": [], "deny": [] }));
        assert!(matches!(err, Err(ConrogateError::PluginConfigInvalid(_))));
    }

    #[test]
    fn validate_rejects_invalid_entry() {
        let p = IpAllowDenyPlugin::new();
        let err = p.validate_config(&json!({ "allow": ["not-an-ip"] }));
        assert!(matches!(err, Err(ConrogateError::PluginConfigInvalid(_))));
    }

    #[test]
    fn validate_accepts_null_and_valid() {
        let p = IpAllowDenyPlugin::new();
        assert!(p.validate_config(&Value::Null).is_ok());
        assert!(p
            .validate_config(&json!({ "deny": ["10.0.0.0/8"] }))
            .is_ok());
        assert!(p.validate_config(&json!({ "allow": ["1.2.3.4"] })).is_ok());
    }

    #[test]
    fn configured_rejects_invalid_config() {
        let p = IpAllowDenyPlugin::new();
        assert!(p.configured(&json!({ "allow": [], "deny": [] })).is_err());
    }
}
