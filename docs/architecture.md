# 当前协调架构

> 说明本文档基于当前代码（v0.0.1）描述的运行时协调架构：进程拓扑、配置下发、遥测上报与内部调度机制。

## 1. 总体架构

Conrogate 采用「控制面 + 数据面」双平面架构，提供三种部署形态：

```text
                    ┌─────────────────────────────────────────────┐
                    │              控制面 (Control Plane)          │
                    │  conrogate-control  /  conrogate 合并进程      │
                    │  ├─ REST API (axum, 默认 :9000)              │
                    │  ├─ 鉴权中间件 (Bearer Token)                │
                    │  ├─ 审计服务 (AuditService)                  │
                    │  ├─ 配置版本管理 (publish/rollback/diff)      │
                    │  └─ 配置快照发布 ──► Redis ConfigCache ─┐     │
                    └──────────────┬──────────────────────────┼────┘
                                   │ 写库 (SeaORM)            │ Pub/Sub
                                   ▼                          ▼
                    ┌──────────────────────────────────────────────┐
                    │  持久层 (MySQL / PostgreSQL / SQLite)        │
                    │  routes / upstreams / plugin_bindings /      │
                    │  config_versions / metrics / events /        │
                    │  audit_logs / node_applications / plugins    │
                    └──────────────────────┬───────────────────────┘
                                           │ 读配置（轮询 / 快照）
                                           ▼
                    ┌──────────────────────────────────────────────┐
                    │              数据面 (Data Plane)              │
                    │  conrogate-gate × N  /  conrogate 合并进程     │
                    │  路由匹配 → 插件链 → 限流/熔断 → 负载均衡 → 转发 │
                    └──────────────┬───────────────────────────────┘
                                   │ 遥测上报（heartbeat / metrics / events）
                                   ▼
                    控制面 /reports/* 端点（分离模式）
```

### 平面职责

| 平面 | 职责 | 二进制 | 端口 |
|------|------|--------|------|
| 控制面 | 管理 API、配置落库、版本发布/回滚、指标入库、审计 | `conrogate-control` | 9000 |
| 数据面 | 请求转发、插件执行、限流/熔断、遥测采集 | `conrogate-gate` | 8080 |
| 合并模式 | 控制面 + 数据面同进程双端口 | `conrogate` | 8080 + 9000 |
| 迁移工具 | 迁移 CLI，部署前置执行 | `conrogate-migrate` | — |

### 部署模式

| 模式 | 进程 | 配置下发路径 | 遥测路径 |
|------|------|-------------|----------|
| 合并模式 | 单进程 | 进程内共享仓储 + 后台热加载任务（同一 DB 连接池） | mpsc 内存通道直写 DB |
| 分离模式 | gate×N + control×1 | gate 直连只读 DB 轮询 / Redis 快照 / HTTP 拉取 | gate → HTTP 上报 → control 落库 |
| HTTP-only | gate（无 DB） | HTTP 定时拉取 control API（`config_source=http`） | 心跳上报（可选） |

## 2. 组件清单

| 组件 | 位置 | 职责 |
|------|------|------|
| `conrogate-core` | 核心层 | 契约（Trait/DTO/Config）、负载均衡、插件框架、协议适配、持久化（Entity/迁移/仓储/配置缓存）、流量治理、控制面服务（`control/`） |
| `conrogate-gateway` | 网关核心 | 路由匹配、上游选择、遥测、健康检查、配置热载 |
| `conrogate-gate` | 数据面二进制 | 独立启动数据面（含心跳上报） |
| `conrogate-control` | 控制面二进制 | 独立启动控制面 |
| `conrogate` | 合并二进制 | Bootstrap 装配两平面 |

## 3. 配置下发协调（核心链路）

### 3.1 写路径（控制面 → 存储）

1. 操作者调用 REST API（如 `POST /routes`）。
2. `ControlService` 调用对应仓储写库（SeaORM）。
3. 每次写操作同步记录审计日志（`audit.log`）。
4. 发布配置：`POST /configs/publish` 汇总当前 路由/上游/插件绑定 为 `ConfigSnapshot`，写 `config_versions` 表生成新版本号。
5. 若配置了 `CONROGATE_GATE_CONFIG_CACHE_REDIS_URL`，将快照写入 Redis（原子管道：`SET version` + `SET snapshot:{v}` + `PUBLISH notify`），并触发本地 watch channel 通知。
   - Redis 写入失败**不阻断**发布，仅告警（`service.rs` `publish_config`）。
6. 回滚：`POST /configs/versions/:v/rollback` 先 `apply_snapshot` 回写业务表，再落版本行、写 Redis 快照。

### 3.2 读路径（存储 → 数据面热加载）

数据面按优先级选择配置来源（`server.rs` / `bootstrap.rs`）：

```text
1. Redis ConfigCache 快照   （单次读取即完整三件套：routes+upstreams+bindings）
        │ 失败降级
2. 直连 DB 轮询             （list_enabled + list_all + list_by_route）
```

- **合并模式**：`config_hot_reload_loop`（bootstrap.rs）后台任务，优先订阅 Redis Pub/Sub，收到通知立即重载；超时或订阅失败则轮询。
- **分离模式**：
  - `config_source=db`：`GatewayServer::from_config_with_db` 内部启动热加载任务（`server.rs:464`）。
  - `config_source=http`：`HttpConfigLoader` 定时拉取 `/api/v1/routes`、`/api/v1/upstreams`、`/api/v1/routes/:id/plugins`（自动翻页）。
- **原子性**：任一数据源读取失败（routes/upstreams/bindings 任一项）即跳过本次重载，**保持当前生效配置**，避免半套配置被刷入导致流量中断。
- 重载动作：`plugin_executor.set_route_chains()` → `route_matcher.load_with_bindings()` → `upstream_selector.load_upstreams()`，三者按序原子替换。

## 4. 遥测上报协调

### 4.1 合并模式（进程内通道）

```text
gate 请求处理
   └─ TelemetryReportImpl ──mpsc(100_000)──► MetricAggregator ──► metric_repo 落库
   └─ TelemetryReportImpl ──mpsc(100_000)──► event-consumer ──► event_repo 落库
                                              （批量 ≥batch_size 或定时 flush）
```

- 指标按 `bucket_sec` 聚合桶聚合后批量 upsert。
- 事件批量插入，出错仅告警不阻塞请求路径。

### 4.2 分离模式（HTTP 上报）

gate 每 30s 上报心跳 `POST /api/v1/reports/heartbeat`（gate_id + version + timestamp），control 端 upsert `node_applications`。

> 控制面受保护路由（管理 + 上报端点）统一挂载在 `api_prefix`（默认 `/api/v1`）之下，公开路由（`/health`、`/healthz`、`/readyz`、`/openapi.json`）保留根路径；gate 侧通过 `CONROGATE_GATE_REFRESH_CONTROL_API_PREFIX` 对齐同一前缀。
> 心跳上报携带的 `timestamp` 持久化为 `node_applications.last_seen`，`list_stale` 基于 `last_seen` 判定过期节点。

## 5. 进程内协调（合并模式 Bootstrap）

`conrogate/src/bootstrap.rs` 装配顺序：

1. DB 连接池（main 读写 + read 只读）。
2. 初始化 9 类仓储，加载初始配置到内存。
3. 组装数据面组件链（BalancerRegistry → 健康检查 → 限流/熔断 → 插件 → 路由匹配 → 遥测）。
4. **启动数据面**：`tokio::spawn` GatewayServer，通过 broadcast channel 接收停机信号。
5. **启动控制面**：`tokio::spawn` axum 服务（共享同一批仓储 Arc）。
6. **后台任务**：`TaskManager` 管理 config-hot-reload / metric-aggregator / event-consumer，逆序取消。
7. **优雅停机**：main 收到 SIGINT → broadcast 通知 gate → 等待宽限期（long_conn_drain + 5s）→ TaskManager.shutdown(10s)。

关键同步原语：

| 原语 | 用途 |
|------|------|
| `broadcast::Sender<()>` | 停机信号分发（main → gate → 后台任务） |
| `mpsc::channel(100_000)` | 遥测数据流（metric/event 从请求线程到落库任务） |
| `Arc<dyn Repo>` | 控制面与数据面共享仓储实例 |
| `watch::channel<u64>` | Redis 配置版本变更的进程内通知 |
| `RwLock<Arc<Config>>` | 配置热载（ConfigReloader） |

## 6. 鉴权与安全协调

- 公开路由：`/health`、`/healthz`、`/readyz`、`/openapi.json`。
- 受保护路由：所有管理 + 上报端点，通过 `Authorization: Bearer <token>` 校验。
- `CONROGATE_CONTROL_AUTH_TOKEN` 支持逗号分隔的多个 `operator:secret:role` token，请求携带任一匹配即通过，角色由命中 token 自身决定；无 `:role` 段的 token 回退 `viewer`。
- `CONROGATE_CONTROL_AUTH_TOKEN` 为空字符串时鉴权中间件放行（无鉴权模式）。
- gate 上报 / 拉取配置时携带同一 token（`CONROGATE_GATE_REFRESH_CONTROL_API_TOKEN`）。

## 8. 关键文件索引

| 文件 | 协调职责 |
|------|----------|
| `conrogate/src/bootstrap.rs` | 合并模式全量装配、热加载循环、停机编排 |
| `conrogate-gate/src/main.rs` | 分离模式数据面启动、心跳上报任务 |
| `conrogate-control/src/main.rs` | 分离模式控制面启动、仓储组装 |
| `conrogate-gateway/src/server.rs` | 数据面配置快照加载（Redis 优先 + DB 降级） |
| `conrogate-gate/src/http_config_loader.rs` | HTTP 模式配置拉取（翻页 + 原子重载） |
| `conrogate-core/src/control/service.rs` | 配置发布/回滚 + Redis 快照写入 |
| `conrogate-core/src/storage/config_cache.rs` | ConfigCache 抽象：DB 直读实现 / Redis 实现（原子管道 + Pub/Sub） |
| `conrogate-core/src/control/api.rs` | 控制面路由注册与鉴权分层 |
