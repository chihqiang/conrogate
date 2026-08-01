//! 配置结构体与加载。

use crate::error::ConrogateError;
use crate::protocol::ProtocolId;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 全局配置结构体
#[derive(Debug, Clone)]
pub struct Config {
    pub common: CommonConfig,
    pub gate: GateConfig,
    pub node: NodeConfig,
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

#[derive(Debug, Clone)]
pub struct RefreshConfig {
    pub config_poll_interval: Duration,
    pub config_source: String,
    pub control_api_url: String,
    pub control_api_token: String,
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
pub struct NodeConfig {
    pub auto_migrate: bool,
    pub seed_demo: bool,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            auto_migrate: false,
            seed_demo: false,
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

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub token: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub name: String,
    pub username: String,
    pub password: String,
    pub ssl_mode: String,
    pub read_host: String,
    pub max_connections: u32,
    pub connect_timeout: Duration,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 5432,
            name: "conrogate".into(),
            username: "conrogate".into(),
            password: String::new(),
            ssl_mode: "prefer".into(),
            read_host: String::new(),
            max_connections: 10,
            connect_timeout: Duration::from_secs(5),
        }
    }
}

impl DbConfig {
    pub fn database_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}?ssl-mode={}",
            self.username, self.password, self.host, self.port, self.name, self.ssl_mode
        )
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
            _ => Err(ConrogateError::ConfigInvalid(format!("unknown config source: {s}"))),
        }
    }
}

// ── 环境变量加载 ──

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
        let db_password = env_str("CONROGATE_DB_PASSWORD", "");
        let config_cache_redis_url = env_str("CONROGATE_GATE_CONFIG_CACHE_REDIS_URL", "");

        let rate_limit_mode = env_str("CONROGATE_GATE_RATE_LIMIT_MODE", "local");
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
            None
        };

        let config = Config {
            common: CommonConfig {
                instance_id: env_str("CONROGATE_INSTANCE_ID", ""),
            },
            gate: GateConfig {
                listen: GateListenConfig {
                    host: env_str("CONROGATE_GATE_HOST", "0.0.0.0"),
                    port: env_u16("CONROGATE_GATE_PORT", 8080),
                    protocol: std::env::var("CONROGATE_GATE_PROTOCOL")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(ProtocolId::Http),
                    tls: TlsConfig {
                        enabled: env_bool("CONROGATE_GATE_TLS_ENABLED", false),
                        mode: env_str("CONROGATE_GATE_TLS_MODE", "terminate"),
                        cert_file: env_str("CONROGATE_GATE_TLS_CERT_FILE", ""),
                        key: env_str("CONROGATE_GATE_TLS_KEY", ""),
                    },
                    trusted_proxies: env_list("CONROGATE_GATE_TRUSTED_PROXIES"),
                },
                worker_threads: env_usize("CONROGATE_GATE_WORKER_THREADS", 0),
                connection: ConnectionConfig {
                    max_connections: env_usize("CONROGATE_GATE_MAX_CONNECTIONS", 10_000),
                    max_body_bytes: env_usize("CONROGATE_GATE_MAX_BODY_BYTES", 10_485_760),
                    max_header_bytes: env_usize("CONROGATE_GATE_MAX_HEADER_BYTES", 65_536),
                    idle_timeout: env_duration_ms("CONROGATE_GATE_IDLE_TIMEOUT_MS", 30_000),
                },
                upstream_pool: UpstreamPoolConfig {
                    max_idle_conns: env_usize("CONROGATE_GATE_UPSTREAM_MAX_IDLE_CONNS", 128),
                    idle_timeout: env_duration_ms(
                        "CONROGATE_GATE_UPSTREAM_IDLE_TIMEOUT_MS",
                        60_000,
                    ),
                },
                timeouts: TimeoutConfig {
                    connect: env_duration_ms("CONROGATE_GATE_TIMEOUT_CONNECT_MS", 3000),
                    total: env_duration_ms("CONROGATE_GATE_TIMEOUT_TOTAL_MS", 30_000),
                    read: env_duration_ms("CONROGATE_GATE_TIMEOUT_READ_MS", 15_000),
                },
                retry: RetryConfig {
                    max_attempts: env_u32("CONROGATE_GATE_RETRY_MAX_ATTEMPTS", 2),
                    base_jitter: env_duration_ms("CONROGATE_GATE_RETRY_BASE_JITTER_MS", 50),
                },
                rate_limit: RateLimitConfig {
                    enabled: env_bool("CONROGATE_GATE_RATE_LIMIT_ENABLED", false),
                    mode: rate_limit_mode,
                    global_qps: env_u32("CONROGATE_GATE_RATE_LIMIT_GLOBAL_QPS", 1000),
                    route_qps: env_u32("CONROGATE_GATE_RATE_LIMIT_ROUTE_QPS", 200),
                    ip_qps: env_u32("CONROGATE_GATE_RATE_LIMIT_IP_QPS", 100),
                    conn_qps: env_u32("CONROGATE_GATE_RATE_LIMIT_CONN_QPS", 0),
                    bandwidth_kbps: env_u32("CONROGATE_GATE_RATE_LIMIT_BANDWIDTH_KBPS", 0),
                    cluster_store,
                },
                breaker: BreakerConfig {
                    enabled: env_bool("CONROGATE_GATE_BREAKER_ENABLED", false),
                    window: env_duration_ms("CONROGATE_GATE_BREAKER_WINDOW_MS", 10_000),
                    failure_rate_threshold: env_f64(
                        "CONROGATE_GATE_BREAKER_FAILURE_RATE_THRESHOLD",
                        0.5,
                    ),
                    min_requests: env_u32("CONROGATE_GATE_BREAKER_MIN_REQUESTS", 10),
                    wait: env_duration_ms("CONROGATE_GATE_BREAKER_WAIT_MS", 30_000),
                    half_open_max: env_u32("CONROGATE_GATE_BREAKER_HALF_OPEN_MAX", 5),
                },
                shutdown: ShutdownConfig {
                    long_conn_drain: env_duration_ms(
                        "CONROGATE_GATE_SHUTDOWN_LONG_CONN_DRAIN_MS",
                        30_000,
                    ),
                },
                refresh: RefreshConfig {
                    config_poll_interval: env_duration_ms(
                        "CONROGATE_GATE_REFRESH_CONFIG_POLL_INTERVAL_MS",
                        5000,
                    ),
                    config_source: env_str("CONROGATE_GATE_REFRESH_CONFIG_SOURCE", "db"),
                    control_api_url: env_str("CONROGATE_GATE_REFRESH_CONTROL_API_URL", ""),
                    control_api_token: env_str(
                        "CONROGATE_GATE_REFRESH_CONTROL_API_TOKEN",
                        "",
                    ),
                    config_cache_redis_url,
                    config_cache_connect_timeout: env_duration_ms(
                        "CONROGATE_GATE_CONFIG_CACHE_REDIS_CONNECT_TIMEOUT_MS",
                        2000,
                    ),
                    config_cache_snapshot_retention: env_u32(
                        "CONROGATE_GATE_CONFIG_CACHE_SNAPSHOT_RETENTION",
                        10,
                    ),
                },
                upgrade: UpgradeConfig {
                    buffer_size: env_usize("CONROGATE_GATE_UPGRADE_BUFFER_SIZE_BYTES", 65_536),
                    idle_timeout: env_duration_ms(
                        "CONROGATE_GATE_UPGRADE_IDLE_TIMEOUT_MS",
                        300_000,
                    ),
                },
                telemetry: TelemetryConfig {
                    batch_size: env_usize("CONROGATE_GATE_TELEMETRY_BATCH_SIZE", 1000),
                    batch_interval: env_duration_ms(
                        "CONROGATE_GATE_TELEMETRY_BATCH_INTERVAL_MS",
                        1000,
                    ),
                    buffer_max_messages: env_usize(
                        "CONROGATE_GATE_TELEMETRY_BUFFER_MAX_MESSAGES",
                        100_000,
                    ),
                    db_retry_backoff: env_duration_ms(
                        "CONROGATE_GATE_TELEMETRY_DB_RETRY_BACKOFF_MS",
                        500,
                    ),
                    db_retry_max_backoff: env_duration_ms(
                        "CONROGATE_GATE_TELEMETRY_DB_RETRY_MAX_BACKOFF_MS",
                        30_000,
                    ),
                    bucket_sec: env_u32("CONROGATE_GATE_TELEMETRY_BUCKET_SEC", 10),
                },
            },
            node: NodeConfig {
                auto_migrate: env_bool("CONROGATE_NODE_AUTO_MIGRATE", false),
                seed_demo: env_bool("CONROGATE_NODE_SEED_DEMO", false),
            },
            control: ControlConfig {
                listen: ControlListenConfig {
                    enabled: env_bool("CONROGATE_CONTROL_LISTEN_ENABLED", true),
                    host: env_str("CONROGATE_CONTROL_LISTEN_HOST", "0.0.0.0"),
                    port: env_u16("CONROGATE_CONTROL_LISTEN_PORT", 9000),
                    api_prefix: env_str("CONROGATE_CONTROL_LISTEN_API_PREFIX", "/api/v1"),
                    tls: TlsConfig {
                        enabled: env_bool("CONROGATE_CONTROL_LISTEN_TLS_ENABLED", false),
                        mode: "terminate".into(),
                        cert_file: env_str("CONROGATE_CONTROL_LISTEN_TLS_CERT_FILE", ""),
                        key: env_str("CONROGATE_CONTROL_LISTEN_TLS_KEY", ""),
                    },
                },
                auth: AuthConfig {
                    token: env_str("CONROGATE_CONTROL_AUTH_TOKEN", ""),
                },
            },
            db: DbConfig {
                host: env_str("CONROGATE_DB_HOST", "127.0.0.1"),
                port: env_u16("CONROGATE_DB_PORT", 5432),
                name: env_str("CONROGATE_DB_NAME", "conrogate"),
                username: env_str("CONROGATE_DB_USER", "conrogate"),
                password: db_password,
                ssl_mode: env_str("CONROGATE_DB_SSL_MODE", "prefer"),
                read_host: env_str("CONROGATE_DB_READ_HOST", ""),
                max_connections: env_u32("CONROGATE_DB_MAX_CONNECTIONS", 10),
                connect_timeout: env_duration_ms("CONROGATE_DB_CONNECT_TIMEOUT_MS", 5000),
            },
            log: LogConfig {
                level: env_str("CONROGATE_LOG_LEVEL", "info"),
                format: env_str("CONROGATE_LOG_FORMAT", "json"),
                console: env_bool("CONROGATE_LOG_OUTPUT_CONSOLE", true),
                file_enabled: env_bool("CONROGATE_LOG_OUTPUT_FILE_ENABLED", true),
                file_path: env_str(
                    "CONROGATE_LOG_OUTPUT_FILE_PATH",
                    "/var/log/conrogate/conrogate.log",
                ),
                rotation_size_mb: env_u32("CONROGATE_LOG_OUTPUT_FILE_ROTATION_SIZE_MB", 100)
                    as u64,
                retention_days: env_u32("CONROGATE_LOG_OUTPUT_FILE_RETENTION_DAYS", 7),
            },
        };

        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConrogateError> {
        if self.db.password.is_empty() {
            return Err(ConrogateError::ConfigInvalid(
                "CONROGATE_DB_PASSWORD is required".into(),
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
            },
            node: NodeConfig::default(),
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

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.validate().is_err()); // password is empty
    }

    #[test]
    fn test_config_with_password() {
        let mut config = Config::default();
        config.db.password = "test".into();
        assert!(config.validate().is_ok());
    }
}
