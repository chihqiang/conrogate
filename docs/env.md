# 环境变量参考

> 所有变量均可通过 `.env` 文件配置（`dotenvy` 自动加载，优先级：命令行参数 > `.env` > 环境变量 > 默认值）。

## 通用

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_INSTANCE_ID` | `String` | `""` | 实例标识，用于遥测和日志 |
| `CONROGATE_GATE_ID` | `String` | `HOSTNAME`（回退 `conrogate`） | 网关标识，区分多网关部署 |
| `HOSTNAME` | `String` | `conrogate` | 容器主机名（`CONROGATE_GATE_ID` 回退） |

## 数据库

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_DB_URL` | `String` | `""`（必填） | 主库完整连接 URL，前缀决定方言：`mysql://`、`postgres://`、`sqlite://` |
| `CONROGATE_DB_READ_URL` | `String` | `""` | 只读库 URL；不设置时回退 `CONROGATE_DB_URL` |
| `CONROGATE_DB_MAX_CONNECTIONS` | `u32` | `10` | 连接池最大连接数 |
| `CONROGATE_DB_CONNECT_TIMEOUT_MS` | `Duration` | `5000` | 连接超时（毫秒） |

### 常用 URL 示例

```
mysql://user:password@127.0.0.1:3306/conrogate
postgres://user:password@127.0.0.1:5432/conrogate
sqlite:///tmp/conrogate.sqlite
sqlite::memory:
```

## 日志

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_LOG_LEVEL` | `String` | `info` | 日志级别：`trace` / `debug` / `info` / `warn` / `error` |
| `CONROGATE_LOG_FORMAT` | `String` | `json` | 格式：`json` / `text` |
| `CONROGATE_LOG_OUTPUT_CONSOLE` | `bool` | `true` | 是否输出到标准输出 |
| `CONROGATE_LOG_OUTPUT_FILE_ENABLED` | `bool` | `true` | 是否输出到文件 |
| `CONROGATE_LOG_OUTPUT_FILE_PATH` | `String` | `/var/log/conrogate/conrogate.log` | 日志文件路径 |
| `CONROGATE_LOG_OUTPUT_FILE_ROTATION_SIZE_MB` | `u32` | `100` | 单文件轮转大小（MB） |
| `CONROGATE_LOG_OUTPUT_FILE_RETENTION_DAYS` | `u32` | `7` | 日志保留天数 |

## 数据面（Gate）

### 监听

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_HOST` | `String` | `0.0.0.0` | 监听地址 |
| `CONROGATE_GATE_PORT` | `u16` | `8080` | 监听端口 |
| `CONROGATE_GATE_PROTOCOL` | `ProtocolId` | `Http` | 协议：`Http` / `Http2` |
| `CONROGATE_GATE_WORKER_THREADS` | `usize` | `0`（自动） | tokio 工作线程数，0 为 CPU 核数 |

### TLS

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_TLS_ENABLED` | `bool` | `false` | 是否启用 TLS |
| `CONROGATE_GATE_TLS_MODE` | `String` | `terminate` | TLS 模式：`terminate`（终止）/ `passthrough`（透传） |
| `CONROGATE_GATE_TLS_CERT_FILE` | `String` | `""` | 证书文件路径 |
| `CONROGATE_GATE_TLS_KEY` | `String` | `""` | 私钥文件路径 |

### 代理

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_TRUSTED_PROXIES` | `List` | `[]` | 可信代理 IP（逗号分隔） |

### 连接

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_MAX_CONNECTIONS` | `usize` | `10000` | 最大并发连接数 |
| `CONROGATE_GATE_MAX_BODY_BYTES` | `usize` | `10485760`（10MB） | 请求体最大字节数 |
| `CONROGATE_GATE_MAX_HEADER_BYTES` | `usize` | `65536`（64KB） | 请求头最大字节数 |
| `CONROGATE_GATE_IDLE_TIMEOUT_MS` | `Duration` | `30000` | 空闲连接超时（毫秒） |

### 上游连接池

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_UPSTREAM_MAX_IDLE_CONNS` | `usize` | `128` | 上游空闲连接池大小 |
| `CONROGATE_GATE_UPSTREAM_IDLE_TIMEOUT_MS` | `Duration` | `60000` | 上游空闲连接超时（毫秒） |

### 超时

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_TIMEOUT_CONNECT_MS` | `Duration` | `3000` | 连接上游超时（毫秒） |
| `CONROGATE_GATE_TIMEOUT_TOTAL_MS` | `Duration` | `30000` | 请求总超时（毫秒） |
| `CONROGATE_GATE_TIMEOUT_READ_MS` | `Duration` | `15000` | 等待响应超时（毫秒） |

### 重试

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_RETRY_MAX_ATTEMPTS` | `u32` | `2` | 最大重试次数（含首次请求） |
| `CONROGATE_GATE_RETRY_BASE_JITTER_MS` | `Duration` | `50` | 重试基础抖动（毫秒） |

### 限流（Rate Limit）

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_RATE_LIMIT_ENABLED` | `bool` | `false` | 是否启用限流 |
| `CONROGATE_GATE_RATE_LIMIT_MODE` | `String` | `local` | 模式：`local`（单机）/ `cluster`（集群，需 Redis） |
| `CONROGATE_GATE_RATE_LIMIT_GLOBAL_QPS` | `u32` | `1000` | 全局 QPS 上限 |
| `CONROGATE_GATE_RATE_LIMIT_ROUTE_QPS` | `u32` | `200` | 单路由 QPS 上限 |
| `CONROGATE_GATE_RATE_LIMIT_IP_QPS` | `u32` | `100` | 单 IP QPS 上限 |
| `CONROGATE_GATE_RATE_LIMIT_CONN_QPS` | `u32` | `0`（不限） | 单连接 QPS 上限 |
| `CONROGATE_GATE_RATE_LIMIT_BANDWIDTH_KBPS` | `u32` | `0`（不限） | 带宽上限（KB/s） |
| `CONROGATE_GATE_RATE_LIMIT_REDIS_URL` | `String` | `""` | Redis URL（`mode=cluster` 时必填） |
| `CONROGATE_GATE_RATE_LIMIT_REDIS_CONNECT_TIMEOUT_MS` | `Duration` | `2000` | Redis 连接超时（毫秒） |

### 熔断器（Circuit Breaker）

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_BREAKER_ENABLED` | `bool` | `false` | 是否启用熔断 |
| `CONROGATE_GATE_BREAKER_MODE` | `String` | `local` | 模式：`local` / `cluster`（需 Redis） |
| `CONROGATE_GATE_BREAKER_WINDOW_MS` | `Duration` | `10000` | 统计窗口（毫秒） |
| `CONROGATE_GATE_BREAKER_FAILURE_RATE_THRESHOLD` | `f64` | `0.5` | 失败率阈值（0.0~1.0） |
| `CONROGATE_GATE_BREAKER_MIN_REQUESTS` | `u32` | `10` | 窗口内最小请求数才触发判断 |
| `CONROGATE_GATE_BREAKER_WAIT_MS` | `Duration` | `30000` | 熔断持续时间（毫秒） |
| `CONROGATE_GATE_BREAKER_HALF_OPEN_MAX` | `u32` | `5` | 半开状态最大探测请求数 |
| `CONROGATE_GATE_BREAKER_REDIS_URL` | `String` | `""` | Redis URL（`mode=cluster` 时必填） |
| `CONROGATE_GATE_BREAKER_REDIS_CONNECT_TIMEOUT_MS` | `Duration` | `2000` | Redis 连接超时（毫秒） |

### 优雅关闭

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_SHUTDOWN_LONG_CONN_DRAIN_MS` | `Duration` | `30000` | 长连接排空超时（毫秒） |

### 配置热刷新

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_REFRESH_CONFIG_POLL_INTERVAL_MS` | `Duration` | `5000` | 配置轮询间隔（毫秒） |
| `CONROGATE_GATE_REFRESH_CONFIG_SOURCE` | `String` | `db` | 配置来源：`db` / `redis` / `http` |
| `CONROGATE_GATE_REFRESH_CONTROL_API_URL` | `String` | `""` | 控制面 API URL（`http` 来源时使用） |
| `CONROGATE_GATE_REFRESH_CONTROL_API_TOKEN` | `String` | `""` | 控制面 API Token |
| `CONROGATE_GATE_CONFIG_CACHE_REDIS_URL` | `String` | `""` | 配置缓存 Redis URL |
| `CONROGATE_GATE_CONFIG_CACHE_REDIS_CONNECT_TIMEOUT_MS` | `Duration` | `2000` | Redis 连接超时（毫秒） |
| `CONROGATE_GATE_CONFIG_CACHE_SNAPSHOT_RETENTION` | `u32` | `10` | 快照保留份数 |

### WebSocket/HTTP 升级

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_UPGRADE_BUFFER_SIZE_BYTES` | `usize` | `65536`（64KB） | 升级缓冲区大小 |
| `CONROGATE_GATE_UPGRADE_IDLE_TIMEOUT_MS` | `Duration` | `300000`（5min） | 升级空闲超时（毫秒） |

### 遥测（Telemetry）

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_TELEMETRY_BATCH_SIZE` | `usize` | `1000` | 批量写入大小 |
| `CONROGATE_GATE_TELEMETRY_BATCH_INTERVAL_MS` | `Duration` | `1000` | 批量写入间隔（毫秒） |
| `CONROGATE_GATE_TELEMETRY_BUFFER_MAX_MESSAGES` | `usize` | `100000` | 缓冲区最大消息数 |
| `CONROGATE_GATE_TELEMETRY_DB_RETRY_BACKOFF_MS` | `Duration` | `500` | 写库重试退避（毫秒） |
| `CONROGATE_GATE_TELEMETRY_DB_RETRY_MAX_BACKOFF_MS` | `Duration` | `30000` | 写库重试最大退避（毫秒） |
| `CONROGATE_GATE_TELEMETRY_BUCKET_SEC` | `u32` | `10` | 指标聚合桶大小（秒） |

### 出站 TLS

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_GATE_OUTBOUND_TLS_SKIP_VERIFY` | `bool` | `false` | 跳过上游证书校验（仅非生产环境） |

## 节点（Node）

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_NODE_AUTO_MIGRATE` | `bool` | `false` | 启动时自动执行数据库迁移 |
| `CONROGATE_NODE_SEED_DEMO` | `bool` | `false` | 启动时写入演示数据 |

## 控制面（Control）

### 监听

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_CONTROL_ENABLED` | `bool` | `true` | 是否启用控制面 |
| `CONROGATE_CONTROL_LISTEN_HOST` | `String` | `0.0.0.0` | 监听地址 |
| `CONROGATE_CONTROL_LISTEN_PORT` | `u16` | `9000` | 监听端口 |
| `CONROGATE_CONTROL_API_PREFIX` | `String` | `/api/v1` | API 路由前缀 |
| `CONROGATE_CONTROL_AUTH_TOKEN` | `String` | `""` | 鉴权 Token（空字符串时无鉴权） |

### 控制面 TLS

| 变量 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `CONROGATE_CONTROL_TLS_ENABLED` | `bool` | `false` | 是否启用 TLS |
| `CONROGATE_CONTROL_TLS_MODE` | `String` | `terminate` | TLS 模式 |
| `CONROGATE_CONTROL_TLS_CERT_FILE` | `String` | `""` | 证书文件路径 |
| `CONROGATE_CONTROL_TLS_KEY` | `String` | `""` | 私钥文件路径 |

## 变量类型说明

| 类型 | 格式 |
|------|------|
| `String` | 任意字符串 |
| `bool` | `true` / `1` / `yes` = true，其余 = false |
| `u16` / `u32` / `usize` | 数字字符串（如 `"8080"`） |
| `f64` | 浮点数字符串（如 `"0.5"`） |
| `Duration` | 毫秒数字字符串（如 `"5000"` = 5 秒） |
| `List` | 逗号分隔字符串（如 `"127.0.0.1,10.0.0.0/8"`） |
