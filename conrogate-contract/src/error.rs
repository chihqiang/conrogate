//! Conrogate 统一错误类型。

use thiserror::Error;

/// 全局唯一错误类型
#[derive(Debug, Error)]
pub enum ConrogateError {
    // ---- 通用（10000 段）----
    #[error("参数非法: {0}")]
    BadRequest(String),
    #[error("未认证")]
    Unauthorized,
    #[error("无权限")]
    Forbidden,
    #[error("资源不存在: {0}")]
    NotFound(String),
    #[error("资源已存在: {0}")]
    Conflict(String),
    #[error("请求过于频繁")]
    RateLimited,
    #[error("请求体过大")]
    PayloadTooLarge,
    #[error("业务处理失败: {message}")]
    Business { code: i32, message: String },

    // ---- 配置/路由（20000 段）----
    #[error("路由不存在: {0}")]
    RouteNotFound(String),
    #[error("上游不存在: {0}")]
    UpstreamNotFound(String),
    #[error("插件配置非法: {0}")]
    PluginConfigInvalid(String),
    #[error("插件未找到: {0}")]
    PluginNotFound(String),
    #[error("插件运行时异常: {0}")]
    PluginRuntime(String),
    #[error("配置校验失败: {0}")]
    ConfigInvalid(String),
    #[error("配置发布并发冲突")]
    ConfigConcurrencyConflict,

    // ---- 数据层（30000 段）----
    #[error("数据库错误")]
    DatabaseInternal,
    #[error("数据转换错误: {0}")]
    DataMapping(String),
    #[error("数据库迁移错误: {0}")]
    Migration(String),

    // ---- 网关转发（40000 段）----
    #[error("网络/代理内部错误")]
    NetworkInternal,
    #[error("上游连接超时")]
    UpstreamTimeout,
    #[error("上游连接失败: {0}")]
    UpstreamConnectFailed(String),
    #[error("上游响应异常: {0}")]
    UpstreamBadResponse(String),
    #[error("协议不支持: {0}")]
    ProtocolNotSupported(String),
    #[error("网关内部错误")]
    GatewayInternal,
    #[error("服务熔断中")]
    CircuitBreakerOpen,
    #[error("请求被限流拦截")]
    Limited,
    #[error("请求重试耗尽: {0}")]
    RetryExhausted(String),

    // ---- 系统（50000 段）----
    #[error("配置加载失败: {0}")]
    ConfigLoad(String),
    #[error("初始化失败: {0}")]
    Init(String),
    #[error("内部错误: {0}")]
    Internal(String),
}

impl ConrogateError {
    /// 错误码映射
    pub fn code(&self) -> i32 {
        match self {
            Self::BadRequest(_) => 10001,
            Self::Unauthorized => 10002,
            Self::Forbidden => 10003,
            Self::NotFound(_) => 10004,
            Self::Conflict(_) => 10005,
            Self::RateLimited => 10006,
            Self::PayloadTooLarge => 10007,
            Self::Business { code, .. } => *code,

            Self::RouteNotFound(_) => 20001,
            Self::UpstreamNotFound(_) => 20002,
            Self::PluginConfigInvalid(_) => 20003,
            Self::PluginNotFound(_) => 20004,
            Self::PluginRuntime(_) => 20005,
            Self::ConfigInvalid(_) => 20006,
            Self::ConfigConcurrencyConflict => 20007,

            Self::DatabaseInternal => 30001,
            Self::DataMapping(_) => 30002,
            Self::Migration(_) => 30003,

            Self::NetworkInternal => 40001,
            Self::UpstreamTimeout => 40002,
            Self::UpstreamConnectFailed(_) => 40003,
            Self::UpstreamBadResponse(_) => 40004,
            Self::ProtocolNotSupported(_) => 40005,
            Self::GatewayInternal => 40006,
            Self::CircuitBreakerOpen => 40007,
            Self::Limited => 40008,
            Self::RetryExhausted(_) => 40009,

            Self::ConfigLoad(_) => 50001,
            Self::Init(_) => 50002,
            Self::Internal(_) => 59999,
        }
    }

    /// 是否为内部错误（不对外暴露细节）
    pub fn is_internal(&self) -> bool {
        matches!(
            self,
            Self::DatabaseInternal
                | Self::DataMapping(_)
                | Self::Migration(_)
                | Self::NetworkInternal
                | Self::GatewayInternal
                | Self::ConfigLoad(_)
                | Self::Init(_)
                | Self::Internal(_)
        )
    }
}
