//! 公共常量。

/// 默认分页大小
pub const DEFAULT_PAGE_SIZE: u32 = 20;

/// 最大分页大小
pub const MAX_PAGE_SIZE: u32 = 200;

/// 默认配置轮询周期（毫秒）
pub const DEFAULT_CONFIG_POLL_INTERVAL_MS: u64 = 5000;

/// 默认指标入库桶大小（秒）
pub const DEFAULT_TELEMETRY_BUCKET_SEC: u32 = 10;

/// 默认批量上报条数
pub const DEFAULT_TELEMETRY_BATCH_SIZE: usize = 1000;

/// 默认批量上报周期（毫秒）
pub const DEFAULT_TELEMETRY_BATCH_INTERVAL_MS: u64 = 1000;

/// 默认进程内缓冲上限
pub const DEFAULT_TELEMETRY_BUFFER_MAX: usize = 100_000;

/// trace_id 请求头
pub const TRACE_ID_HEADER: &str = "X-Trace-Id";

/// request_id 请求头
pub const REQUEST_ID_HEADER: &str = "X-Request-Id";

/// 默认数据面端口
pub const DEFAULT_GATE_PORT: u16 = 8080;

/// 默认控制面端口
pub const DEFAULT_CONTROL_PORT: u16 = 9000;

/// Redis 配置缓存键前缀
pub const REDIS_CONFIG_VERSION_KEY: &str = "conrogate:config:version";

/// Redis 配置快照键前缀
pub const REDIS_CONFIG_SNAPSHOT_KEY: &str = "conrogate:config:snapshot";

/// Redis 配置变更通知频道
pub const REDIS_CONFIG_NOTIFY_CHANNEL: &str = "conrogate:config:notify";
