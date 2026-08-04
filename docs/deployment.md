# 部署指南

> 本文档基于当前代码（v0.0.1），描述 Conrogate 的本地开发、容器化构建与生产部署流程。

## 1. 架构选择

| 场景 | 模式 | 进程 | 端口 |
|------|------|------|------|
| 本地开发/测试 | 合并模式 | `conrogate` × 1 | 8080 + 9000 |
| 小规模生产 | 合并模式 | `conrogate` × N（多副本） | 8080 + 9000 |
| 大规模生产 | 分离模式 | `conrogate-gate` × N + `conrogate-control` × 1~2 | 8080 / 9000 |

## 2. 基础依赖

### MySQL（推荐）

```bash
docker run -d --name conrogate-mysql \
  -e MYSQL_ROOT_PASSWORD=rootpass \
  -e MYSQL_DATABASE=conrogate \
  -e MYSQL_USER=conrogate \
  -e MYSQL_PASSWORD=conrogatepass \
  -p 3306:3306 \
  mysql:8.0
```

### PostgreSQL（备选）

```bash
docker run -d --name conrogate-pg \
  -e POSTGRES_USER=conrogate \
  -e POSTGRES_PASSWORD=conrogate_dev \
  -e POSTGRES_DB=conrogate \
  -p 5432:5432 \
  postgres:16
```

### SQLite（开发/测试）

无需外部服务，直接使用本地文件路径或内存数据库：

```bash
export CONROGATE_DB_URL='sqlite:///tmp/conrogate.sqlite'
# 或内存数据库（重启后数据丢失）
export CONROGATE_DB_URL='sqlite::memory:'
```

> 注意：SQLite 文件必须由进程自身创建，`create_if_missing(true)` 已在 `conrogate-storage/src/pool.rs` 中实现。SQLite 无需 Redis 即可运行。

### Redis（可选，用于配置缓存与集群限流）

```bash
docker run -d --name conrogate-redis \
  -p 6379:6379 \
  redis:7-alpine
```

> 不配置 Redis 时：配置下发降级为直连 DB 轮询；限流/熔断仅支持本地模式（`mode=local`）。

### Docker Compose 一键启动依赖

```bash
docker compose -f docker-compose.deps.yml up -d
# 启动 PostgreSQL 16 + Redis 7，等待 healthcheck 通过
```

## 3. 本地构建

### 前提条件

- Rust ≥ 1.85（edition 2024 支持）
- OpenSSL 开发库（macOS：`brew install openssl`，Linux：`apt install libssl-dev`）
- 支持的数据库客户端库已就绪（MySQL/PostgreSQL/SQLite 均通过 SeaORM 驱动内置）

### 构建四个二进制

```bash
# 全量构建（debug）
cargo build -p conrogate -p conrogate-gate -p conrogate-control -p conrogate-migrate

# 全量构建（release，含 LTO 优化）
cargo build --release -p conrogate -p conrogate-gate -p conrogate-control -p conrogate-migrate
```

> `release` 构建启用 `lto = "thin"` + `codegen-units = 1`，二进制体积约 30-40MB。

### 一键开发脚本

```bash
./scripts/dev-up.sh
# 自动：启动依赖（PG + Redis）→ 执行迁移 → 启动合并模式（seed 演示数据）
```

脚本使用的环境变量（硬编码）：

```bash
CONROGATE_DB_PASSWORD=conrogate_dev   # 未被代码使用（URL 已包含密码）
CONROGATE_NODE_AUTO_MIGRATE=false
CONROGATE_NODE_SEED_DEMO=true
CONROGATE_CONTROL_AUTH_TOKEN=admin:dev-token:admin
```

## 4. 数据库迁移

### 手动迁移

```bash
CONROGATE_DB_URL='mysql://conrogate:conrogatepass@127.0.0.1:3306/conrogate' \
  cargo run -p conrogate-migrate
```

迁移工具自动按方言加锁串行化（防多实例并发迁移）：

| 方言 | 锁机制 |
|------|--------|
| PostgreSQL | `SELECT pg_advisory_lock(20260101)` |
| MySQL | `SELECT GET_LOCK('conrogate_migrate', 10)` |
| SQLite | 引擎自身串行写保证，无 advisory lock |

### 自动迁移

合并模式与控制面二进制支持 `auto_migrate` 开关，启动时自动执行迁移：

```bash
export CONROGATE_NODE_AUTO_MIGRATE=true   # 启动时自动迁移（生产环境建议 false）
```

### 数据库表清单

| 表名 | 用途 |
|------|------|
| `upstreams` | 上游组 |
| `upstream_nodes` | 上游节点（IP + 权重 + 健康状态） |
| `routes` | 路由规则（匹配条件 + 优先级） |
| `route_plugin_bindings` | 路由 ↔ 插件绑定（含插件配置 JSON） |
| `config_versions` | 配置版本快照（发布/回滚审计） |
| `metric_aggregates` | 指标聚合桶（QPS / 延迟 / 状态码分布） |
| `gateway_events` | 事件流（请求级别事件，如超时、熔断触发） |
| `audit_logs` | 操作审计日志（所有写操作留痕） |
| `node_applications` | 网关节点心跳（gate_id + 最后上报时间戳） |
| `installed_plugins` | 已安装插件状态管理 |

## 5. 合并模式部署（开发/小规模）

### 直接运行

```bash
CONROGATE_DB_URL='mysql://conrogate:conrogatepass@127.0.0.1:3306/conrogate' \
CONROGATE_NODE_AUTO_MIGRATE=false \
CONROGATE_NODE_SEED_DEMO=true \
CONROGATE_LOG_OUTPUT_FILE_ENABLED=false \
CONROGATE_CONTROL_AUTH_TOKEN=admin:dev-token:admin \
./target/release/conrogate
```

启动日志确认两个端口同时监听：

```
gate_port = 8080
control_port = 9000
"starting conrogate (merged mode)"
```

### 验证

```bash
# 控制面健康检查（无需鉴权）
curl http://127.0.0.1:9000/health
curl http://127.0.0.1:9000/healthz
curl http://127.0.0.1:9000/readyz

# 数据面访问 demo 路由（seed 后可用）
curl http://127.0.0.1:8080/demo/hello

# 控制面 API（需鉴权）
curl -H "Authorization: Bearer admin:dev-token:admin" \
  http://127.0.0.1:9000/routes
```

### 关键环境变量（合并模式最小配置）

```bash
CONROGATE_DB_URL='...'                     # 必填
CONROGATE_LOG_OUTPUT_FILE_ENABLED=false    # 容器内无 /var/log 权限时必设 false
CONROGATE_CONTROL_AUTH_TOKEN=''            # 空字符串 = 关闭鉴权（开发环境）
```

## 6. 分离模式部署（生产规模）

### 架构拓扑

```
                     ┌──────────────────────────────────────┐
                     │         MySQL / PostgreSQL            │
                     │      (主库读写 / 可选只读从库)         │
                     └──────────┬──────────────────────┬────┘
                                │ 主库读写              │ 只读库
                    ┌───────────▼──────────┐   ┌───────▼──────────────┐
                    │  conrogate-control    │   │  conrogate-gate × N  │
                    │  ┌──────────────────┐ │   │  ┌────────────────┐  │
                    │  │ REST API :9000   │ │   │  │ HTTP :8080     │  │
                    │  │ 审计/版本/指标    │ │   │  │ 路由/插件/转发   │  │
                    │  └──────────────────┘ │   │  └────────────────┘  │
                    │  ┌──────────────────┐ │   │  心跳上报 → :9000    │
                    │  │ Redis ConfigCache│◄├───┤  配置拉取 ← DB/Redis │
                    │  └──────────────────┘ │   └──────────────────────┘
                    └──────────────────────┘
```

### 控制面启动

```bash
CONROGATE_DB_URL='mysql://conrogate:conrogatepass@127.0.0.1:3306/conrogate' \
CONROGATE_CONTROL_LISTEN_PORT=9000 \
CONROGATE_CONTROL_AUTH_TOKEN=your-secret-token \
CONROGATE_GATE_CONFIG_CACHE_REDIS_URL='redis://127.0.0.1:6379' \
./target/release/conrogate-control
```

### 数据面启动（DB 轮询模式，默认）

```bash
CONROGATE_DB_READ_URL='mysql://readonly:ro-pw@slave.example.com:3306/conrogate' \
CONROGATE_GATE_PORT=8080 \
CONROGATE_GATE_REFRESH_CONFIG_SOURCE=db \
CONROGATE_GATE_REFRESH_CONFIG_POLL_INTERVAL_MS=5000 \
CONROGATE_GATE_CONFIG_CACHE_REDIS_URL='redis://127.0.0.1:6379' \
./target/release/conrogate-gate
```

配置热加载优先级：Redis 快照 → 直连只读 DB 轮询（任一步骤失败跳过本次重载，保持当前配置）。

### 数据面启动（HTTP 拉取模式，无 DB 直连）

```bash
CONROGATE_GATE_REFRESH_CONFIG_SOURCE=http \
CONROGATE_GATE_REFRESH_CONTROL_API_URL=http://control:9000 \
CONROGATE_GATE_REFRESH_CONTROL_API_TOKEN=your-secret-token \
CONROGATE_GATE_CONFIG_CACHE_REDIS_URL='' \
./target/release/conrogate-gate
```

### 数据面启动（SQLite 单机模式）

```bash
# SQLite 无需额外依赖，无 Redis 时自动降级为 DB 轮询
CONROGATE_DB_URL='sqlite:///data/conrogate.sqlite' \
CONROGATE_GATE_CONFIG_CACHE_REDIS_URL='' \
./target/release/conrogate-gate
```

> SQLite 路径必须可写（容器内推荐 `/tmp` 或挂载 volume）。`create_if_missing` 已自动启用。

### 心跳上报（分离模式）

`conrogate-gate` 启动后每 30s 向控制面上报心跳 `POST /api/v1/reports/heartbeat`（gate_id + version + timestamp），控制面 upsert `node_applications` 表并将上报 `timestamp` 持久化到 `last_seen`，作为节点活跃判定依据。

上报前提：`CONROGATE_GATE_REFRESH_CONTROL_API_URL` 非空；前缀需与控制面 `CONROGATE_CONTROL_LISTEN_API_PREFIX` 保持一致（`CONROGATE_GATE_REFRESH_CONTROL_API_PREFIX`，默认 `/api/v1`）。

## 7. 容器化部署

### 构建镜像

```bash
docker build -t conrogate:latest .
```

Dockerfile 构建要点：

| 阶段 | 基础镜像 | 说明 |
|------|---------|------|
| builder | `rust:1.88-bookworm` | 多阶段编译，含 OpenSSL dev |
| runtime | `debian:bookworm-slim` | 最小运行时，ca-certificates + libssl3 |

镜像内二进制：`conrogate`、`conrogate-gate`、`conrogate-control`、`conrogate-migrate` 均位于 `/app/`。

运行用户：`conrogate`（非 root）。

### 容器内运行

```bash
# 默认合并模式（ENTRYPOINT 已设置）
docker run -d --name conrogate \
  -e CONROGATE_DB_URL='mysql://conrogate:conrogatepass@host.docker.internal:3306/conrogate' \
  -e CONROGATE_LOG_OUTPUT_FILE_ENABLED=false \
  -e CONROGATE_CONTROL_AUTH_TOKEN=admin:dev-token:admin \
  -p 8080:8080 -p 9000:9000 \
  conrogate:latest
```

```bash
# 指定分离模式运行 gate
docker run -d --name conrogate-gate \
  -e CONROGATE_GATE_REFRESH_CONFIG_SOURCE=http \
  -e CONROGATE_GATE_REFRESH_CONTROL_API_URL=http://control-host:9000 \
  -e CONROGATE_GATE_REFRESH_CONTROL_API_TOKEN=your-token \
  -e CONROGATE_LOG_OUTPUT_FILE_ENABLED=false \
  -p 8080:8080 \
  conrogate:latest \
  /app/conrogate-gate
```

```bash
# 指定分离模式运行 control
docker run -d --name conrogate-control \
  -e CONROGATE_DB_URL='mysql://conrogate:conrogatepass@host.docker.internal:3306/conrogate' \
  -e CONROGATE_CONTROL_AUTH_TOKEN=your-secret-token \
  -e CONROGATE_GATE_CONFIG_CACHE_REDIS_URL='redis://host.docker.internal:6379' \
  -e CONROGATE_LOG_OUTPUT_FILE_ENABLED=false \
  -p 9000:9000 \
  conrogate:latest \
  /app/conrogate-control
```

> 容器内无 `/var/log` 写入权限，**必须**设置 `CONROGATE_LOG_OUTPUT_FILE_ENABLED=false`（默认值为 `true`，会报错）。

### 执行迁移容器

```bash
docker run --rm \
  -e CONROGATE_DB_URL='mysql://conrogate:conrogatepass@host.docker.internal:3306/conrogate' \
  conrogate:latest \
  /app/conrogate-migrate
```

## 8. 环境变量配置

详见 `docs/env.md`，生产环境最小变量清单：

### 必填

| 变量 | 值 |
|------|----|
| `CONROGATE_DB_URL` | 完整数据库连接 URL（`mysql://` / `postgres://` / `sqlite://`） |
| `CONROGATE_LOG_OUTPUT_FILE_ENABLED` | `false`（容器内）或路径权限已就绪 |

### 生产推荐

| 变量 | 推荐值 | 说明 |
|------|--------|------|
| `CONROGATE_CONTROL_AUTH_TOKEN` | 非空随机字符串 | 鉴权 token |
| `CONROGATE_GATE_CONFIG_CACHE_REDIS_URL` | Redis URL | 配置缓存 + Pub/Sub 推送 |
| `CONROGATE_NODE_AUTO_MIGRATE` | `false` | 生产环境由迁移工具独立执行 |
| `CONROGATE_GATE_RATE_LIMIT_ENABLED` | `true` | 启用限流保护 |
| `CONROGATE_GATE_BREAKER_ENABLED` | `true` | 启用熔断保护 |
| `CONROGATE_DB_READ_URL` | 只读库 URL | 读写分离，数据面只读连接不占用主库连接池 |
| `CONROGATE_LOG_LEVEL` | `warn` | 生产环境降低日志噪音 |

## 9. 运行时约束与已知注意事项

| 事项 | 说明 |
|------|------|
| Rust 版本 | 必须 ≥ 1.85（workspace 使用 `edition 2024`，`rust:1.88-bookworm` 已验证通过） |
| SQLite 文件创建 | `pool.rs` 已配置 `create_if_missing(true)`，但路径目录必须存在且可写 |
| SQLite `/tmp` 路径 | 容器内 `/tmp` 可写，推荐用于 SQLite 单机部署 |
| 配置热加载一致性 | Redis 写入失败自动重试（3 次 × 200ms），仍失败则 `invalidate()` 删除版本键，数据面降级为 DB 轮询（秒级恢复） |
| 日志目录 | 默认 `/var/log/conrogate/conrogate.log`，容器内无写权限需关闭文件日志或挂载 volume |
| WebSocket 空闲 | 默认 5 分钟超时（`CONROGATE_GATE_UPGRADE_IDLE_TIMEOUT_MS=300000`），生产环境按需调整 |

## 10. 一键部署命令速查

```bash
# ── 本地开发（SQLite + 合并模式，最简）──
CONROGATE_DB_URL='sqlite::memory:' \
CONROGATE_LOG_OUTPUT_FILE_ENABLED=false \
cargo run -p conrogate

# ── 本地开发（MySQL + Redis + 合并模式）──
./scripts/dev-up.sh

# ── 容器化部署（合并模式）──
docker build -t conrogate:latest .
docker run -d -p 8080:8080 -p 9000:9000 \
  -e CONROGATE_DB_URL='mysql://conrogate:conrogatepass@mysql:3306/conrogate' \
  -e CONROGATE_LOG_OUTPUT_FILE_ENABLED=false \
  conrogate:latest

# ── 生产部署（分离模式）──
# 1. 控制面
docker run -d -p 9000:9000 \
  -e CONROGATE_DB_URL='mysql://conrogate:pass@mysql:3306/conrogate' \
  -e CONROGATE_CONTROL_AUTH_TOKEN=$SECRET \
  -e CONROGATE_GATE_CONFIG_CACHE_REDIS_URL='redis://redis:6379' \
  conrogate:latest /app/conrogate-control

# 2. 数据面 × N
for i in $(seq 1 3); do
  docker run -d -p $((8080+i)):8080 \
    -e CONROGATE_DB_READ_URL='mysql://readonly:ro@slave:3306/conrogate' \
    -e CONROGATE_GATE_CONFIG_CACHE_REDIS_URL='redis://redis:6379' \
    -e CONROGATE_GATE_REFRESH_CONTROL_API_URL='http://control:9000' \
    conrogate:latest /app/conrogate-gate
done
```
