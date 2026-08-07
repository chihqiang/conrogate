//! Conrogate 统一错误类型。
//!
//! 业务错误码唯一权威定义处：所有状态码以 `ConrogateError::ERR_*` 关联常量形式集中在此，
//! 控制面/数据面/插件均引用常量，禁止散落字面量。响应体构建见 `contract/response.rs`。

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
    // ── 业务错误码（唯一权威定义）──

    /// 成功
    pub const OK: i32 = 0;
    // 通用（10000 段）
    /// 请求参数非法
    pub const ERR_BAD_REQUEST: i32 = 10001;
    /// 未认证（缺少/无效 Token）
    pub const ERR_UNAUTHORIZED: i32 = 10002;
    /// 无权限（控制面 RBAC）/ IP 被拒（数据面黑名单、ip_allow_deny）
    pub const ERR_FORBIDDEN: i32 = 10003;
    /// 资源不存在
    pub const ERR_NOT_FOUND: i32 = 10004;
    /// 资源冲突（重复创建）
    pub const ERR_CONFLICT: i32 = 10005;
    /// 请求过于频繁
    pub const ERR_RATE_LIMITED: i32 = 10006;
    /// 请求体过大
    pub const ERR_PAYLOAD_TOO_LARGE: i32 = 10007;
    /// 请求体读取失败（数据面缓冲模式）
    pub const ERR_BODY_READ: i32 = 10008;
    /// 请求体读取超时（数据面缓冲模式）
    pub const ERR_BODY_READ_TIMEOUT: i32 = 10009;
    // 配置/路由（20000 段）
    /// 路由不存在
    pub const ERR_ROUTE_NOT_FOUND: i32 = 20001;
    /// 上游不存在
    pub const ERR_UPSTREAM_NOT_FOUND: i32 = 20002;
    /// 插件配置非法
    pub const ERR_PLUGIN_CONFIG_INVALID: i32 = 20003;
    /// 插件未找到
    pub const ERR_PLUGIN_NOT_FOUND: i32 = 20004;
    /// 插件运行时异常
    pub const ERR_PLUGIN_RUNTIME: i32 = 20005;
    /// 配置校验失败
    pub const ERR_CONFIG_INVALID: i32 = 20006;
    /// 配置发布并发冲突
    pub const ERR_CONFIG_CONCURRENCY_CONFLICT: i32 = 20007;
    // 数据层（30000 段）
    /// 数据库错误
    pub const ERR_DATABASE_INTERNAL: i32 = 30001;
    /// 数据转换错误
    pub const ERR_DATA_MAPPING: i32 = 30002;
    /// 数据库迁移错误
    pub const ERR_MIGRATION: i32 = 30003;
    // 网关转发（40000 段）
    /// 网络/代理内部错误
    pub const ERR_NETWORK_INTERNAL: i32 = 40001;
    /// 上游连接超时
    pub const ERR_UPSTREAM_TIMEOUT: i32 = 40002;
    /// 上游连接失败
    pub const ERR_UPSTREAM_CONNECT_FAILED: i32 = 40003;
    /// 上游响应异常
    pub const ERR_UPSTREAM_BAD_RESPONSE: i32 = 40004;
    /// 协议不支持
    pub const ERR_PROTOCOL_NOT_SUPPORTED: i32 = 40005;
    /// 网关内部错误
    pub const ERR_GATEWAY_INTERNAL: i32 = 40006;
    /// 服务熔断中
    pub const ERR_CIRCUIT_BREAKER_OPEN: i32 = 40007;
    /// 请求被限流拦截
    pub const ERR_LIMITED: i32 = 40008;
    /// 请求重试耗尽
    pub const ERR_RETRY_EXHAUSTED: i32 = 40009;
    // 系统（50000 段）
    /// 配置加载失败 / 服务未就绪
    pub const ERR_CONFIG_LOAD: i32 = 50001;
    /// 初始化失败
    pub const ERR_INIT: i32 = 50002;
    /// 内部错误（兜底）
    pub const ERR_INTERNAL: i32 = 59999;

    /// 错误码映射
    pub fn code(&self) -> i32 {
        match self {
            Self::BadRequest(_) => Self::ERR_BAD_REQUEST,
            Self::Unauthorized => Self::ERR_UNAUTHORIZED,
            Self::Forbidden => Self::ERR_FORBIDDEN,
            Self::NotFound(_) => Self::ERR_NOT_FOUND,
            Self::Conflict(_) => Self::ERR_CONFLICT,
            Self::RateLimited => Self::ERR_RATE_LIMITED,
            Self::PayloadTooLarge => Self::ERR_PAYLOAD_TOO_LARGE,
            Self::Business { code, .. } => *code,

            Self::RouteNotFound(_) => Self::ERR_ROUTE_NOT_FOUND,
            Self::UpstreamNotFound(_) => Self::ERR_UPSTREAM_NOT_FOUND,
            Self::PluginConfigInvalid(_) => Self::ERR_PLUGIN_CONFIG_INVALID,
            Self::PluginNotFound(_) => Self::ERR_PLUGIN_NOT_FOUND,
            Self::PluginRuntime(_) => Self::ERR_PLUGIN_RUNTIME,
            Self::ConfigInvalid(_) => Self::ERR_CONFIG_INVALID,
            Self::ConfigConcurrencyConflict => Self::ERR_CONFIG_CONCURRENCY_CONFLICT,

            Self::DatabaseInternal => Self::ERR_DATABASE_INTERNAL,
            Self::DataMapping(_) => Self::ERR_DATA_MAPPING,
            Self::Migration(_) => Self::ERR_MIGRATION,

            Self::NetworkInternal => Self::ERR_NETWORK_INTERNAL,
            Self::UpstreamTimeout => Self::ERR_UPSTREAM_TIMEOUT,
            Self::UpstreamConnectFailed(_) => Self::ERR_UPSTREAM_CONNECT_FAILED,
            Self::UpstreamBadResponse(_) => Self::ERR_UPSTREAM_BAD_RESPONSE,
            Self::ProtocolNotSupported(_) => Self::ERR_PROTOCOL_NOT_SUPPORTED,
            Self::GatewayInternal => Self::ERR_GATEWAY_INTERNAL,
            Self::CircuitBreakerOpen => Self::ERR_CIRCUIT_BREAKER_OPEN,
            Self::Limited => Self::ERR_LIMITED,
            Self::RetryExhausted(_) => Self::ERR_RETRY_EXHAUSTED,

            Self::ConfigLoad(_) => Self::ERR_CONFIG_LOAD,
            Self::Init(_) => Self::ERR_INIT,
            Self::Internal(_) => Self::ERR_INTERNAL,
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
