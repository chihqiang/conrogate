//! 插件系统 Trait 定义。

use crate::error::ConrogateError;
use crate::protocol::ProtocolId;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// 插件实现形态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// 编译进二进制的静态插件
    Native,
    /// WASM 在线插件（扩展点，当前不实现）
    Wasm,
}

/// 插件状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Installed,
    Active,
    Disabled,
    Uninstalled,
}

/// 插件上下文：贯穿单请求/会话生命周期。
/// 协议无关字段始终存在；协议特有字段按 protocol 填充。
#[derive(Debug)]
pub struct PluginContext {
    pub request_id: String,
    pub trace_id: String,
    pub route_id: u64,
    pub client_ip: String,
    pub protocol: ProtocolId,

    /// HTTP 系协议（Http / WebSocket 升级前阶段）时填充
    pub http: Option<HttpContext>,

    /// 隧道协议（TcpTunnel）时填充
    pub tunnel: Option<TunnelContext>,

    /// 插件服务访问（仅钩子执行期间有效，由管线执行器注入）
    pub services: PluginServices,
}

/// HTTP 请求上下文
#[derive(Debug)]
pub struct HttpContext {
    pub method: http::Method,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: http::HeaderMap,
    pub body: Option<bytes::Bytes>,
}

/// 隧道连接上下文
#[derive(Debug)]
pub struct TunnelContext {
    pub remote_addr: String,
    pub sni: Option<String>,
    pub alpn: Option<String>,
    pub listen_port: u16,
}

/// 插件执行结果
#[derive(Debug)]
pub enum PluginOutcome {
    /// 放行，继续执行后续逻辑
    Continue,
    /// 直接终止，使用指定响应返回客户端（仅 HTTP 系协议有效）
    Terminate(http::StatusCode, Value),
}

/// 供 after_response 修改的响应视图
#[derive(Debug)]
pub struct PluginResponse {
    pub status: u16,
    pub headers: http::HeaderMap,
    pub body: bytes::Bytes,
}

/// 插件可访问的宿主服务（轻量，不暴露完整 ServiceContext）
#[derive(Clone)]
pub struct PluginServices {
    /// 上报自定义指标（走聚合落库链路）
    pub metrics: Arc<dyn PluginMetrics>,
    /// 结构化日志（挂载当前 trace span）
    pub logger: Arc<dyn PluginLogger>,
}

impl std::fmt::Debug for PluginServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginServices").finish()
    }
}

/// 插件指标上报接口
#[async_trait]
pub trait PluginMetrics: Send + Sync {
    async fn increment(&self, name: &str);
    async fn gauge(&self, name: &str, value: f64);
}

/// 插件日志接口
#[async_trait]
pub trait PluginLogger: Send + Sync {
    async fn log(&self, level: &str, message: &str);
}

/// 插件核心 Trait（双轨共用逻辑契约）
#[async_trait]
pub trait Plugin: Send + Sync {
    /// 插件名称（全局唯一，注册表键）
    fn name(&self) -> &'static str;

    /// 插件实现形态：Native | Wasm
    fn kind(&self) -> PluginKind;

    /// 插件支持的协议列表（控制插件仅在匹配协议时执行）
    fn protocols(&self) -> &[ProtocolId];

    /// 是否阻断性：true 时抛错/Terminate 将终止请求链路
    fn is_blocking(&self) -> bool;

    /// 是否需要读取请求体：true 时网关以缓冲模式载入 body 供插件访问；
    /// false 时网关以流式模式透传 body，不载入内存。
    /// 默认 false — 大多数插件（鉴权、CORS、日志）只需 header 信息。
    fn requires_body(&self) -> bool {
        false
    }

    /// 插件配置校验（安装/绑定时执行）
    fn validate_config(&self, config: &Value) -> Result<(), ConrogateError>;

    /// 全局初始化（每实例一次）
    async fn init(&self, config: &Value) -> Result<(), ConrogateError> {
        let _ = config;
        Ok(())
    }

    // ── HTTP 系协议钩子 ──

    /// 请求转发前执行（HTTP / WebSocket 升级前阶段）
    async fn before_request(
        &self,
        ctx: &mut PluginContext,
    ) -> Result<PluginOutcome, ConrogateError>;

    /// 响应回包前执行（默认透传）
    async fn after_response(
        &self,
        ctx: &mut PluginContext,
        resp: &mut PluginResponse,
    ) -> Result<(), ConrogateError> {
        let _ = (ctx, resp);
        Ok(())
    }

    // ── 隧道协议钩子 ──

    /// 连接建立时执行（默认放行）
    async fn on_connect(
        &self,
        ctx: &mut PluginContext,
    ) -> Result<PluginOutcome, ConrogateError> {
        let _ = ctx;
        Ok(PluginOutcome::Continue)
    }

    /// 连接断开时执行（默认无操作）
    async fn on_disconnect(&self, ctx: &mut PluginContext) -> Result<(), ConrogateError> {
        let _ = ctx;
        Ok(())
    }

    // ── 生命周期 ──

    /// 卸载/停机资源释放
    async fn shutdown(&self) -> Result<(), ConrogateError> {
        Ok(())
    }
}
