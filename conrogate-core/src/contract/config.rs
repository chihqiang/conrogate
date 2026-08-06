//! 配置结构体与加载。

use crate::contract::error::ConrogateError;
use crate::contract::protocol::ProtocolId;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 全局配置结构体
#[derive(Debug, Clone)]
pub struct Config {
    pub common: CommonConfig,
    pub gate: GateConfig,
    pub control: ControlConfig,
    pub db: DbConfig,
    pub log: LogConfig,
}

#[derive(Debug, Clone)]
pub struct CommonConfig {
    pub instance_id: String,
}

#[derive(Debug, Clone)]
pub struct GateConfig {
    pub listen: GateListenConfig,
    pub worker_threads: usize,
    pub connection: ConnectionConfig,
    pub upstream_pool: UpstreamPoolConfig,
    pub timeouts: TimeoutConfig,
    pub retry: RetryConfig,
    pub rate_limit: RateLimitConfig,
    pub breaker: BreakerConfig,
    pub shutdown: ShutdownConfig,
    pub refresh: RefreshConfig,
    pub upgrade: UpgradeConfig,
    pub telemetry: TelemetryConfig,
    pub outbound_tls: OutboundTlsConfig,
    /// 网关实例标识（用于遥测区分多网关部署），默认取主机名
    pub gate_id: String,
}

#[derive(Debug, Clone)]
pub struct GateListenConfig {
    pub host: String,
    pub port: u16,
    pub protocol: ProtocolId,
    pub tls: TlsConfig,
    pub trusted_proxies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub enabled: bool,
    pub mode: String,
    pub cert_file: String,
    pub key: String,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "terminate".into(),
            cert_file: String::new(),
            key: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub max_connections: usize,
    pub max_body_bytes: usize,
    pub max_header_bytes: usize,
    pub idle_timeout: Duration,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_connections: 10_000,
            max_body_bytes: 10_485_760,
            max_header_bytes: 65_536,
            idle_timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpstreamPoolConfig {
    pub max_idle_conns: usize,
    pub idle_timeout: Duration,
}

impl Default for UpstreamPoolConfig {
    fn default() -> Self {
        Self {
            max_idle_conns: 128,
            idle_timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    pub connect: Duration,
    pub total: Duration,
    pub read: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(3),
            total: Duration::from_secs(30),
            read: Duration::from_secs(15),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_jitter: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 2,
            base_jitter: Duration::from_millis(50),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub mode: String,
    pub global_qps: u32,
    pub route_qps: u32,
    pub ip_qps: u32,
    pub conn_qps: u32,
    pub bandwidth_kbps: u32,
    pub cluster_store: Option<RedisStoreConfig>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "local".into(),
            global_qps: 1000,
            route_qps: 200,
            ip_qps: 100,
            conn_qps: 0,
            bandwidth_kbps: 0,
            cluster_store: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RedisStoreConfig {
    pub redis_url: String,
    pub connect_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct BreakerConfig {
    pub enabled: bool,
    pub window: Duration,
    pub failure_rate_threshold: f64,
    pub min_requests: u32,
    pub wait: Duration,
    pub half_open_max: u32,
    /// 集群模式共享计数存储
    pub cluster_store: Option<RedisStoreConfig>,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            window: Duration::from_secs(10),
            failure_rate_threshold: 0.5,
            min_requests: 10,
            wait: Duration::from_secs(30),
            half_open_max: 5,
            cluster_store: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    pub long_conn_drain: Duration,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            long_conn_drain: Duration::from_secs(30),
        }
    }
}

/// 出站 TLS 配置
#[derive(Debug, Clone, Default)]
pub struct OutboundTlsConfig {
    /// 跳过上游证书校验（仅非生产环境，需显式配置 + 告警日志）
    pub skip_verify: bool,
}

#[derive(Debug, Clone)]
pub struct RefreshConfig {
    pub config_poll_interval: Duration,
    pub config_source: String,
    pub control_api_url: String,
    pub control_api_token: String,
    /// 控制面 API 路由前缀（与 CONROGATE_CONTROL_LISTEN_API_PREFIX 保持一致）
    pub control_api_prefix: String,
    pub config_cache_redis_url: String,
    pub config_cache_connect_timeout: Duration,
    pub config_cache_snapshot_retention: u32,
}

impl Default for RefreshConfig {
    fn default() -> Self {
        Self {
            config_poll_interval: Duration::from_secs(5),
            config_source: "db".into(),
            control_api_url: String::new(),
            control_api_token: String::new(),
            control_api_prefix: "/api/v1".into(),
            config_cache_redis_url: String::new(),
            config_cache_connect_timeout: Duration::from_secs(2),
            config_cache_snapshot_retention: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpgradeConfig {
    pub buffer_size: usize,
    pub idle_timeout: Duration,
}

impl Default for UpgradeConfig {
    fn default() -> Self {
        Self {
            buffer_size: 65_536,
            idle_timeout: Duration::from_secs(300),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub batch_size: usize,
    pub batch_interval: Duration,
    pub buffer_max_messages: usize,
    pub db_retry_backoff: Duration,
    pub db_retry_max_backoff: Duration,
    pub bucket_sec: u32,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            batch_interval: Duration::from_secs(1),
            buffer_max_messages: 100_000,
            db_retry_backoff: Duration::from_millis(500),
            db_retry_max_backoff: Duration::from_secs(30),
            bucket_sec: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ControlConfig {
    pub listen: ControlListenConfig,
    pub auth: AuthConfig,
}

#[derive(Debug, Clone)]
pub struct ControlListenConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub api_prefix: String,
    pub tls: TlsConfig,
}

impl Default for ControlListenConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "0.0.0.0".into(),
            port: 9000,
            api_prefix: "/api/v1".into(),
            tls: TlsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct DbConfig {
    /// 主库完整连接 URL（`CONROGATE_DB_URL`），URL 前缀决定方言：
    /// `postgres://` / `mysql://` / `sqlite://`（或 `sqlite::memory:`）
    pub url: String,
    /// 只读库完整连接 URL（`CONROGATE_DB_READ_URL`）；为空时回退到主库 `url`
    pub read_url: String,
    pub max_connections: u32,
    pub connect_timeout: Duration,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            read_url: String::new(),
            max_connections: 10,
            connect_timeout: Duration::from_secs(5),
        }
    }
}

impl DbConfig {
    /// 主库连接 URL（完整 URL 直接生效，不拼接）
    pub fn database_url(&self) -> String {
        self.url.clone()
    }

    /// 只读库连接 URL：优先 `read_url`，其次回退主库 `url`
    pub fn read_database_url(&self) -> String {
        if !self.read_url.is_empty() {
            return self.read_url.clone();
        }
        self.url.clone()
    }
}

#[derive(Debug, Clone)]
pub struct LogConfig {
    pub level: String,
    pub format: String,
    pub console: bool,
    pub file_enabled: bool,
    pub file_path: String,
    pub rotation_size_mb: u64,
    pub retention_days: u32,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            format: "json".into(),
            console: true,
            file_enabled: true,
            file_path: "/var/log/conrogate/conrogate.log".into(),
            rotation_size_mb: 100,
            retention_days: 7,
        }
    }
}

/// 配置缓存来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    Redis,
    Db,
    Http,
}

impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Redis => write!(f, "redis"),
            Self::Db => write!(f, "db"),
            Self::Http => write!(f, "http"),
        }
    }
}

impl std::str::FromStr for ConfigSource {
    type Err = ConrogateError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "redis" => Ok(Self::Redis),
            "db" => Ok(Self::Db),
            "http" => Ok(Self::Http),
            _ => Err(ConrogateError::ConfigInvalid(format!(
                "unknown config source: {s}"
            ))),
        }
    }
}

// ── 环境变量加载 ──

/// 默认网关标识：优先取 CONROGATE_GATE_ID，否则用主机名兜底
fn default_gate_id() -> String {
    std::env::var("CONROGATE_GATE_ID")
        .unwrap_or_else(|_| std::env::var("HOSTNAME").unwrap_or_else(|_| "conrogate".into()))
}

fn env_str(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .and_then(|s| match s.to_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

fn env_u16(key: &str, default: u16) -> u16 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_duration_ms(key: &str, default_ms: u64) -> Duration {
    Duration::from_millis(
        std::env::var(key)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(default_ms),
    )
}

fn env_list(key: &str) -> Vec<String> {
    std::env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(|item| item.trim().to_string()).collect())
        .unwrap_or_default()
}

impl Config {
    pub fn from_env() -> Result<Self, ConrogateError> {
        // 默认值以 Config::default() 为唯一来源，环境变量仅做覆盖；
        // 避免 from_env 与 Default 两处硬编码默认值漂移。
        let def = Config::default();

        // 集群共享状态：mode=cluster 时必须提供 Redis URL
        let rate_limit_mode = env_str("CONROGATE_GATE_RATE_LIMIT_MODE", &def.gate.rate_limit.mode);
        let cluster_store = if rate_limit_mode == "cluster" {
            let redis_url = env_str("CONROGATE_GATE_RATE_LIMIT_REDIS_URL", "");
            if redis_url.is_empty() {
                return Err(ConrogateError::ConfigInvalid(
                    "rate_limit mode=cluster requires CONROGATE_GATE_RATE_LIMIT_REDIS_URL".into(),
                ));
            }
            Some(RedisStoreConfig {
                redis_url,
                connect_timeout: env_duration_ms(
                    "CONROGATE_GATE_RATE_LIMIT_REDIS_CONNECT_TIMEOUT_MS",
                    2000,
                ),
            })
        } else {
            def.gate.rate_limit.cluster_store.clone()
        };

        let breaker_mode = env_str("CONROGATE_GATE_BREAKER_MODE", "local");
        let breaker_cluster_store = if breaker_mode == "cluster" {
            let redis_url = env_str("CONROGATE_GATE_BREAKER_REDIS_URL", "");
            if redis_url.is_empty() {
                return Err(ConrogateError::ConfigInvalid(
                    "breaker mode=cluster requires CONROGATE_GATE_BREAKER_REDIS_URL".into(),
                ));
            }
            Some(RedisStoreConfig {
                redis_url,
                connect_timeout: env_duration_ms(
                    "CONROGATE_GATE_BREAKER_REDIS_CONNECT_TIMEOUT_MS",
                    2000,
                ),
            })
        } else {
            def.gate.breaker.cluster_store.clone()
        };

        let config = Config {
            common: CommonConfig {
                instance_id: env_str("CONROGATE_INSTANCE_ID", &def.common.instance_id),
            },
            gate: GateConfig {
                listen: GateListenConfig {
                    host: env_str("CONROGATE_GATE_HOST", &def.gate.listen.host),
                    port: env_u16("CONROGATE_GATE_PORT", def.gate.listen.port),
                    protocol: std::env::var("CONROGATE_GATE_PROTOCOL")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(def.gate.listen.protocol),
                    tls: TlsConfig {
                        enabled: env_bool(
                            "CONROGATE_GATE_TLS_ENABLED",
                            def.gate.listen.tls.enabled,
                        ),
                        mode: env_str("CONROGATE_GATE_TLS_MODE", &def.gate.listen.tls.mode),
                        cert_file: env_str(
                            "CONROGATE_GATE_TLS_CERT_FILE",
                            &def.gate.listen.tls.cert_file,
                        ),
                        key: env_str("CONROGATE_GATE_TLS_KEY", &def.gate.listen.tls.key),
                    },
                    trusted_proxies: env_list("CONROGATE_GATE_TRUSTED_PROXIES"),
                },
                worker_threads: env_usize("CONROGATE_GATE_WORKER_THREADS", def.gate.worker_threads),
                connection: ConnectionConfig {
                    max_connections: env_usize(
                        "CONROGATE_GATE_MAX_CONNECTIONS",
                        def.gate.connection.max_connections,
                    ),
                    max_body_bytes: env_usize(
                        "CONROGATE_GATE_MAX_BODY_BYTES",
                        def.gate.connection.max_body_bytes,
                    ),
                    max_header_bytes: env_usize(
                        "CONROGATE_GATE_MAX_HEADER_BYTES",
                        def.gate.connection.max_header_bytes,
                    ),
                    idle_timeout: env_duration_ms(
                        "CONROGATE_GATE_IDLE_TIMEOUT_MS",
                        def.gate.connection.idle_timeout.as_millis() as u64,
                    ),
                },
                upstream_pool: UpstreamPoolConfig {
                    max_idle_conns: env_usize(
                        "CONROGATE_GATE_UPSTREAM_MAX_IDLE_CONNS",
                        def.gate.upstream_pool.max_idle_conns,
                    ),
                    idle_timeout: env_duration_ms(
                        "CONROGATE_GATE_UPSTREAM_IDLE_TIMEOUT_MS",
                        def.gate.upstream_pool.idle_timeout.as_millis() as u64,
                    ),
                },
                timeouts: TimeoutConfig {
                    connect: env_duration_ms(
                        "CONROGATE_GATE_TIMEOUT_CONNECT_MS",
                        def.gate.timeouts.connect.as_millis() as u64,
                    ),
                    total: env_duration_ms(
                        "CONROGATE_GATE_TIMEOUT_TOTAL_MS",
                        def.gate.timeouts.total.as_millis() as u64,
                    ),
                    read: env_duration_ms(
                        "CONROGATE_GATE_TIMEOUT_READ_MS",
                        def.gate.timeouts.read.as_millis() as u64,
                    ),
                },
                retry: RetryConfig {
                    max_attempts: env_u32(
                        "CONROGATE_GATE_RETRY_MAX_ATTEMPTS",
                        def.gate.retry.max_attempts,
                    ),
                    base_jitter: env_duration_ms(
                        "CONROGATE_GATE_RETRY_BASE_JITTER_MS",
                        def.gate.retry.base_jitter.as_millis() as u64,
                    ),
                },
                rate_limit: RateLimitConfig {
                    enabled: env_bool(
                        "CONROGATE_GATE_RATE_LIMIT_ENABLED",
                        def.gate.rate_limit.enabled,
                    ),
                    mode: rate_limit_mode,
                    global_qps: env_u32(
                        "CONROGATE_GATE_RATE_LIMIT_GLOBAL_QPS",
                        def.gate.rate_limit.global_qps,
                    ),
                    route_qps: env_u32(
                        "CONROGATE_GATE_RATE_LIMIT_ROUTE_QPS",
                        def.gate.rate_limit.route_qps,
                    ),
                    ip_qps: env_u32(
                        "CONROGATE_GATE_RATE_LIMIT_IP_QPS",
                        def.gate.rate_limit.ip_qps,
                    ),
                    conn_qps: env_u32(
                        "CONROGATE_GATE_RATE_LIMIT_CONN_QPS",
                        def.gate.rate_limit.conn_qps,
                    ),
                    bandwidth_kbps: env_u32(
                        "CONROGATE_GATE_RATE_LIMIT_BANDWIDTH_KBPS",
                        def.gate.rate_limit.bandwidth_kbps,
                    ),
                    cluster_store,
                },
                breaker: BreakerConfig {
                    enabled: env_bool("CONROGATE_GATE_BREAKER_ENABLED", def.gate.breaker.enabled),
                    window: env_duration_ms(
                        "CONROGATE_GATE_BREAKER_WINDOW_MS",
                        def.gate.breaker.window.as_millis() as u64,
                    ),
                    failure_rate_threshold: env_f64(
                        "CONROGATE_GATE_BREAKER_FAILURE_RATE_THRESHOLD",
                        def.gate.breaker.failure_rate_threshold,
                    ),
                    min_requests: env_u32(
                        "CONROGATE_GATE_BREAKER_MIN_REQUESTS",
                        def.gate.breaker.min_requests,
                    ),
                    wait: env_duration_ms(
                        "CONROGATE_GATE_BREAKER_WAIT_MS",
                        def.gate.breaker.wait.as_millis() as u64,
                    ),
                    half_open_max: env_u32(
                        "CONROGATE_GATE_BREAKER_HALF_OPEN_MAX",
                        def.gate.breaker.half_open_max,
                    ),
                    cluster_store: breaker_cluster_store,
                },
                shutdown: ShutdownConfig {
                    long_conn_drain: env_duration_ms(
                        "CONROGATE_GATE_SHUTDOWN_LONG_CONN_DRAIN_MS",
                        def.gate.shutdown.long_conn_drain.as_millis() as u64,
                    ),
                },
                refresh: RefreshConfig {
                    config_poll_interval: env_duration_ms(
                        "CONROGATE_GATE_REFRESH_CONFIG_POLL_INTERVAL_MS",
                        def.gate.refresh.config_poll_interval.as_millis() as u64,
                    ),
                    config_source: env_str(
                        "CONROGATE_GATE_REFRESH_CONFIG_SOURCE",
                        &def.gate.refresh.config_source,
                    ),
                    control_api_url: env_str(
                        "CONROGATE_GATE_REFRESH_CONTROL_API_URL",
                        &def.gate.refresh.control_api_url,
                    ),
                    control_api_token: env_str(
                        "CONROGATE_GATE_REFRESH_CONTROL_API_TOKEN",
                        &def.gate.refresh.control_api_token,
                    ),
                    control_api_prefix: env_str(
                        "CONROGATE_GATE_REFRESH_CONTROL_API_PREFIX",
                        &def.gate.refresh.control_api_prefix,
                    ),
                    config_cache_redis_url: env_str(
                        "CONROGATE_GATE_CONFIG_CACHE_REDIS_URL",
                        &def.gate.refresh.config_cache_redis_url,
                    ),
                    config_cache_connect_timeout: env_duration_ms(
                        "CONROGATE_GATE_CONFIG_CACHE_REDIS_CONNECT_TIMEOUT_MS",
                        def.gate.refresh.config_cache_connect_timeout.as_millis() as u64,
                    ),
                    config_cache_snapshot_retention: env_u32(
                        "CONROGATE_GATE_CONFIG_CACHE_SNAPSHOT_RETENTION",
                        def.gate.refresh.config_cache_snapshot_retention,
                    ),
                },
                upgrade: UpgradeConfig {
                    buffer_size: env_usize(
                        "CONROGATE_GATE_UPGRADE_BUFFER_SIZE_BYTES",
                        def.gate.upgrade.buffer_size,
                    ),
                    idle_timeout: env_duration_ms(
                        "CONROGATE_GATE_UPGRADE_IDLE_TIMEOUT_MS",
                        def.gate.upgrade.idle_timeout.as_millis() as u64,
                    ),
                },
                telemetry: TelemetryConfig {
                    batch_size: env_usize(
                        "CONROGATE_GATE_TELEMETRY_BATCH_SIZE",
                        def.gate.telemetry.batch_size,
                    ),
                    batch_interval: env_duration_ms(
                        "CONROGATE_GATE_TELEMETRY_BATCH_INTERVAL_MS",
                        def.gate.telemetry.batch_interval.as_millis() as u64,
                    ),
                    buffer_max_messages: env_usize(
                        "CONROGATE_GATE_TELEMETRY_BUFFER_MAX_MESSAGES",
                        def.gate.telemetry.buffer_max_messages,
                    ),
                    db_retry_backoff: env_duration_ms(
                        "CONROGATE_GATE_TELEMETRY_DB_RETRY_BACKOFF_MS",
                        def.gate.telemetry.db_retry_backoff.as_millis() as u64,
                    ),
                    db_retry_max_backoff: env_duration_ms(
                        "CONROGATE_GATE_TELEMETRY_DB_RETRY_MAX_BACKOFF_MS",
                        def.gate.telemetry.db_retry_max_backoff.as_millis() as u64,
                    ),
                    bucket_sec: env_u32(
                        "CONROGATE_GATE_TELEMETRY_BUCKET_SEC",
                        def.gate.telemetry.bucket_sec,
                    ),
                },
                outbound_tls: OutboundTlsConfig {
                    skip_verify: env_bool(
                        "CONROGATE_GATE_OUTBOUND_TLS_SKIP_VERIFY",
                        def.gate.outbound_tls.skip_verify,
                    ),
                },
                gate_id: env_str("CONROGATE_GATE_ID", &def.gate.gate_id),
            },
            control: ControlConfig {
                listen: ControlListenConfig {
                    enabled: env_bool(
                        "CONROGATE_CONTROL_LISTEN_ENABLED",
                        def.control.listen.enabled,
                    ),
                    host: env_str("CONROGATE_CONTROL_LISTEN_HOST", &def.control.listen.host),
                    port: env_u16("CONROGATE_CONTROL_LISTEN_PORT", def.control.listen.port),
                    api_prefix: env_str(
                        "CONROGATE_CONTROL_LISTEN_API_PREFIX",
                        &def.control.listen.api_prefix,
                    ),
                    tls: TlsConfig {
                        enabled: env_bool(
                            "CONROGATE_CONTROL_LISTEN_TLS_ENABLED",
                            def.control.listen.tls.enabled,
                        ),
                        mode: env_str(
                            "CONROGATE_CONTROL_LISTEN_TLS_MODE",
                            &def.control.listen.tls.mode,
                        ),
                        cert_file: env_str(
                            "CONROGATE_CONTROL_LISTEN_TLS_CERT_FILE",
                            &def.control.listen.tls.cert_file,
                        ),
                        key: env_str(
                            "CONROGATE_CONTROL_LISTEN_TLS_KEY",
                            &def.control.listen.tls.key,
                        ),
                    },
                },
                auth: AuthConfig {
                    token: env_str("CONROGATE_CONTROL_AUTH_TOKEN", &def.control.auth.token),
                },
            },
            db: DbConfig {
                url: env_str("CONROGATE_DB_URL", &def.db.url),
                read_url: env_str("CONROGATE_DB_READ_URL", &def.db.read_url),
                max_connections: env_u32("CONROGATE_DB_MAX_CONNECTIONS", def.db.max_connections),
                connect_timeout: env_duration_ms(
                    "CONROGATE_DB_CONNECT_TIMEOUT_MS",
                    def.db.connect_timeout.as_millis() as u64,
                ),
            },
            log: LogConfig {
                level: env_str("CONROGATE_LOG_LEVEL", &def.log.level),
                format: env_str("CONROGATE_LOG_FORMAT", &def.log.format),
                console: env_bool("CONROGATE_LOG_OUTPUT_CONSOLE", def.log.console),
                file_enabled: env_bool("CONROGATE_LOG_OUTPUT_FILE_ENABLED", def.log.file_enabled),
                file_path: env_str("CONROGATE_LOG_OUTPUT_FILE_PATH", &def.log.file_path),
                rotation_size_mb: env_u32(
                    "CONROGATE_LOG_OUTPUT_FILE_ROTATION_SIZE_MB",
                    def.log.rotation_size_mb as u32,
                ) as u64,
                retention_days: env_u32(
                    "CONROGATE_LOG_OUTPUT_FILE_RETENTION_DAYS",
                    def.log.retention_days,
                ),
            },
        };

        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConrogateError> {
        // 完整连接 URL 必填；URL 前缀决定方言，不再支持组件拼接
        if self.db.url.is_empty() {
            return Err(ConrogateError::ConfigInvalid(
                "CONROGATE_DB_URL is required".into(),
            ));
        }

        if self.gate.listen.port == 0 {
            return Err(ConrogateError::ConfigInvalid(
                "gate port must be > 0".into(),
            ));
        }

        if self.control.listen.enabled && self.control.listen.port == 0 {
            return Err(ConrogateError::ConfigInvalid(
                "control port must be > 0".into(),
            ));
        }

        if self.gate.rate_limit.enabled
            && self.gate.rate_limit.mode == "cluster"
            && self.gate.rate_limit.cluster_store.is_none()
        {
            return Err(ConrogateError::ConfigInvalid(
                "rate_limit cluster mode requires redis_url".into(),
            ));
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        // 用于测试：不调 from_env，直接获取默认值
        Self {
            common: CommonConfig {
                instance_id: String::new(),
            },
            gate: GateConfig {
                listen: GateListenConfig {
                    host: "0.0.0.0".into(),
                    port: 8080,
                    protocol: ProtocolId::Http,
                    tls: TlsConfig::default(),
                    trusted_proxies: vec![],
                },
                worker_threads: 0,
                connection: ConnectionConfig::default(),
                upstream_pool: UpstreamPoolConfig::default(),
                timeouts: TimeoutConfig::default(),
                retry: RetryConfig::default(),
                rate_limit: RateLimitConfig::default(),
                breaker: BreakerConfig::default(),
                shutdown: ShutdownConfig::default(),
                refresh: RefreshConfig::default(),
                upgrade: UpgradeConfig::default(),
                telemetry: TelemetryConfig::default(),
                outbound_tls: OutboundTlsConfig::default(),
                gate_id: default_gate_id(),
            },
            control: ControlConfig {
                listen: ControlListenConfig::default(),
                auth: AuthConfig::default(),
            },
            db: DbConfig::default(),
            log: LogConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 环境变量测试会并行执行，必须串行化避免互相干扰
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 保存当前 CONROGATE_* 环境变量，测试结束后恢复
    struct EnvGuard(Vec<(String, Option<String>)>);

    impl EnvGuard {
        fn clear() -> EnvGuard {
            let saved: Vec<(String, Option<String>)> = std::env::vars()
                .map(|(k, v)| (k, Some(v)))
                .filter(|(k, _)| k.starts_with("CONROGATE_"))
                .collect();
            for (k, _) in &saved {
                std::env::remove_var(k);
            }
            EnvGuard(saved)
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (k, v) in &self.0 {
                match v {
                    Some(v) => std::env::set_var(k, v),
                    None => std::env::remove_var(k),
                }
            }
        }
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.validate().is_err()); // url is empty
    }

    #[test]
    fn test_config_with_url() {
        let mut config = Config::default();
        config.db.url = "postgres://user:pw@localhost:5432/conrogate".into();
        assert!(config.validate().is_ok());
    }

    /// 无环境变量时 from_env 应与 Default 完全一致（单一默认值来源）
    #[test]
    fn test_from_env_matches_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::clear();

        let parsed = Config::from_env().expect("from_env should succeed");
        let default = Config::default();

        assert_eq!(parsed.common.instance_id, default.common.instance_id);
        assert_eq!(parsed.gate.listen.host, default.gate.listen.host);
        assert_eq!(parsed.gate.listen.port, default.gate.listen.port);
        assert_eq!(parsed.gate.listen.tls.mode, default.gate.listen.tls.mode);
        assert_eq!(
            parsed.gate.connection.max_connections,
            default.gate.connection.max_connections
        );
        assert_eq!(
            parsed.gate.connection.idle_timeout,
            default.gate.connection.idle_timeout
        );
        assert_eq!(parsed.gate.timeouts.connect, default.gate.timeouts.connect);
        assert_eq!(parsed.gate.timeouts.total, default.gate.timeouts.total);
        assert_eq!(parsed.gate.timeouts.read, default.gate.timeouts.read);
        assert_eq!(
            parsed.gate.retry.max_attempts,
            default.gate.retry.max_attempts
        );
        assert_eq!(
            parsed.gate.rate_limit.enabled,
            default.gate.rate_limit.enabled
        );
        assert_eq!(parsed.gate.rate_limit.mode, default.gate.rate_limit.mode);
        assert_eq!(
            parsed.gate.rate_limit.global_qps,
            default.gate.rate_limit.global_qps
        );
        assert_eq!(parsed.gate.breaker.enabled, default.gate.breaker.enabled);
        assert_eq!(parsed.gate.breaker.window, default.gate.breaker.window);
        assert_eq!(
            parsed.gate.breaker.failure_rate_threshold,
            default.gate.breaker.failure_rate_threshold
        );
        assert_eq!(
            parsed.gate.shutdown.long_conn_drain,
            default.gate.shutdown.long_conn_drain
        );
        assert_eq!(
            parsed.gate.refresh.config_source,
            default.gate.refresh.config_source
        );
        assert_eq!(
            parsed.gate.refresh.config_poll_interval,
            default.gate.refresh.config_poll_interval
        );
        assert_eq!(
            parsed.gate.upgrade.buffer_size,
            default.gate.upgrade.buffer_size
        );
        assert_eq!(
            parsed.gate.upgrade.idle_timeout,
            default.gate.upgrade.idle_timeout
        );
        assert_eq!(
            parsed.gate.telemetry.batch_size,
            default.gate.telemetry.batch_size
        );
        assert_eq!(
            parsed.gate.telemetry.bucket_sec,
            default.gate.telemetry.bucket_sec
        );
        assert_eq!(
            parsed.gate.outbound_tls.skip_verify,
            default.gate.outbound_tls.skip_verify
        );
        assert_eq!(parsed.gate.gate_id, default.gate.gate_id);
        assert_eq!(
            parsed.control.listen.enabled,
            default.control.listen.enabled
        );
        assert_eq!(parsed.control.listen.port, default.control.listen.port);
        assert_eq!(parsed.control.auth.token, default.control.auth.token);
        assert_eq!(parsed.db.max_connections, default.db.max_connections);
        assert_eq!(parsed.db.connect_timeout, default.db.connect_timeout);
        assert_eq!(parsed.db.url, default.db.url);
        assert_eq!(parsed.db.read_url, default.db.read_url);
        assert_eq!(parsed.log.level, default.log.level);
        assert_eq!(parsed.log.rotation_size_mb, default.log.rotation_size_mb);
    }

    /// CONROGATE_DB_URL 完整 URL 直接生效
    #[test]
    fn test_db_url_takes_precedence() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::clear();
        std::env::set_var(
            "CONROGATE_DB_URL",
            "postgres://app:pw@db.example.com:5433/argo?sslmode=require",
        );
        let parsed = Config::from_env().expect("from_env should succeed");
        assert_eq!(
            parsed.db.database_url(),
            "postgres://app:pw@db.example.com:5433/argo?sslmode=require"
        );
        // 完整 URL 即可通过校验（无需单独密码/组件）
        assert!(parsed.validate().is_ok());
    }

    /// 只读库 URL 优先级：read_url > url
    #[test]
    fn test_read_db_url_precedence() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::clear();

        // 无 read_url → 回退主库
        std::env::set_var(
            "CONROGATE_DB_URL",
            "postgres://app:pw@db.example.com:5433/argo?sslmode=require",
        );
        let parsed = Config::from_env().expect("from_env should succeed");
        assert_eq!(parsed.db.read_database_url(), parsed.db.database_url());

        // read_url 优先
        std::env::set_var(
            "CONROGATE_DB_READ_URL",
            "postgres://ro:pw@ro.example.com:5433/argo?sslmode=require",
        );
        let parsed = Config::from_env().expect("from_env should succeed");
        assert!(parsed.db.read_database_url().contains("ro.example.com"));
    }

    /// 环境变量覆盖生效（默认值来源仍是 Default）
    #[test]
    fn test_from_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::clear();
        std::env::set_var("CONROGATE_GATE_PORT", "9999");
        std::env::set_var("CONROGATE_GATE_TIMEOUT_TOTAL_MS", "7000");
        let parsed = Config::from_env().expect("from_env should succeed");
        assert_eq!(parsed.gate.listen.port, 9999);
        assert_eq!(parsed.gate.timeouts.total.as_millis() as u64, 7000);
    }

    /// CONROGATE_GATE_ID 覆盖生效
    #[test]
    fn test_gate_id_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::clear();
        std::env::set_var("CONROGATE_GATE_ID", "gw-east-1");
        let parsed = Config::from_env().expect("from_env should succeed");
        assert_eq!(parsed.gate.gate_id, "gw-east-1");
    }

    /// CONROGATE_DB_URL 直接支持 mysql/sqlite 方言（URL 前缀决定）
    #[test]
    fn test_db_url_mysql_sqlite() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::clear();
        std::env::set_var(
            "CONROGATE_DB_URL",
            "mysql://app:pw@db.example.com:3306/argo",
        );
        let parsed = Config::from_env().expect("from_env should succeed");
        assert!(parsed.db.database_url().starts_with("mysql://"));
        assert!(parsed.validate().is_ok());

        std::env::set_var("CONROGATE_DB_URL", "sqlite::memory:");
        let parsed = Config::from_env().expect("from_env should succeed");
        assert!(parsed.db.database_url().starts_with("sqlite:"));
        assert!(parsed.validate().is_ok());
    }
}
