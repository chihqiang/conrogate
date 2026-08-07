//!
//! 访问日志插件（Conrogate 官方内置插件）。
//!
//! - 插件名：`log`
//! - 协议：HTTP、WebSocket
//! - 阻断性：`blocking = false`（只记录日志，永不拦截请求）
//! - 是否需要请求体：否
//!
//! ## 原理
//!
//! 在请求生命周期内通过 `tracing::info!` 向网关日志输出结构化访问日志，附带 `trace_id` / `request_id` 便于链路追踪。包含两条日志：
//!
//! | 阶段 | 钩子 | 输出内容 |
//! | --- | --- | --- |
//! | 请求进入 | `before_request` | `incoming request`：trace_id、request_id、method、path、client_ip |
//! | 请求结束 | `after_response` | `request completed`：trace_id、request_id、status |
//!
//! `skip_paths` 配置用于跳过健康检查等高频路径：命中的路径在 `before_request` 直接放行且**不记录** incoming（after_response 的 completed 日志同样不输出）。
//!
//! 每个绑定拥有独立配置实例，不同路由可配置不同的跳过规则。
//!
//! ## 请求过程
//!
//! ```text
//! 客户端 → 网关
//!  1. 路由匹配
//!  2. log.before_request
//!      ├─ path 命中 skip_paths → 放行，不记录
//!      └─ 未命中               → tracing::info "incoming request" 后放行
//!  3. 转发到上游（日志插件不阻断、不改写任何内容）
//!  4. log.after_response → tracing::info "request completed"（含 status）
//!  5. 返回客户端
//! ```
//!
//! 日志示例（网关 stdout / 日志文件）：
//!
//! ```json
//! {"timestamp":"...","level":"INFO","fields":{"message":"incoming request","trace_id":"...","request_id":"...","method":"GET","path":"/api/users","client_ip":"10.0.0.1"},"target":"conrogate_core::plugins::log"}
//! {"timestamp":"...","level":"INFO","fields":{"message":"request completed","trace_id":"...","request_id":"...","status":200},"target":"conrogate_core::plugins::log"}
//! ```
//!
//! ## 配置
//!
//! 绑定到路由时，`config` 支持以下字段：
//!
//! | 字段 | 类型 | 默认 | 说明 |
//! | --- | --- | --- | --- |
//! | `log_body` | bool | `false` | 预留：是否记录请求体（当前实现未启用） |
//! | `log_headers` | bool | `false` | 预留：是否记录请求头（当前实现未启用） |
//! | `skip_paths` | string[] | `["/healthz","/readyz"]` | 命中前缀则跳过日志记录 |
//!
//! > 当前版本仅记录请求方法 / 路径 / 客户端 IP / 状态码等元信息；`log_body`、`log_headers` 为后续扩展保留字段，配置后暂不影响行为。
//!
//! ## 使用
//!
//! ### 1. 绑定插件到路由
//!
//! ```bash
//! curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
//!   -H 'Content-Type: application/json' \
//!   -d '{
//!     "plugin_name": "log",
//!     "config": {
//!       "log_body": false,
//!       "log_headers": false,
//!       "skip_paths": ["/healthz", "/readyz", "/metrics"]
//!     },
//!     "order": 0,
//!     "blocking": false,
//!     "enabled": true
//!   }'
//! ```
//!
//! 省略 `config`（传 `null`）时使用默认配置（默认跳过 `/healthz`、`/readyz`）：
//!
//! ```bash
//! curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
//!   -H 'Content-Type: application/json' \
//!   -d '{
//!     "plugin_name": "log",
//!     "config": null,
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
//! curl http://<网关>:8080/your/path
//! # 观察网关日志中出现 incoming request / request completed 两条记录
//! ```
//!
//! ## 注意事项
//!
//! - 日志插件不阻断请求、不读取请求体，可与 auth 等插件同时绑定，按 `order` 升序执行。
//! - 多条插件链中日志输出顺序即绑定 `order` 顺序。
//!
//!
//! Conrogate 官方日志插件：请求访问日志记录。

use crate::contract::{
    plugin::{Plugin, PluginContext, PluginOutcome, PluginResponse},
    protocol::ProtocolId,
    ConrogateError,
};
use async_trait::async_trait;
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

    fn kind(&self) -> crate::contract::plugin::PluginKind {
        crate::contract::plugin::PluginKind::Native
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

    fn configured(&self, config: &Value) -> Result<std::sync::Arc<dyn Plugin>, ConrogateError> {
        if config.is_null() {
            return Ok(std::sync::Arc::new(LogPlugin::new()));
        }
        let cfg: LogPluginConfig = serde_json::from_value(config.clone())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))?;
        Ok(std::sync::Arc::new(LogPlugin { config: cfg }))
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
