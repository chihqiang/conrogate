# Conrogate 操作指南

> 面向运维/接入人员的操作手册：服务启动后如何完成网关配置、发布、观测与回滚。
> 接口细节见 [`docs/api.md`](api.md)，环境变量见 [`docs/env.md`](env.md)。

## 0. 前置：启动服务

按部署方式先启动一个可用的服务（详细命令见 README「开发」与 [`docs/deployment.md`](deployment.md)）：

- **合并模式**（单进程）：`cargo run -p conrogate`（或运行 `./scripts/dev-up.sh`）。
- **分离模式**：`conrogate-migrate`（先迁移）→ `conrogate-control`（9000）+ `conrogate-gate` × N（8080）。
- **本地零依赖模板**：`cp .env.example .env && cargo run -p conrogate`（SQLite + 自动迁移 + 演示数据）。

启动后先做体检：

```bash
curl http://127.0.0.1:9000/healthz      # 控制面存活
curl http://127.0.0.1:8080/readyz       # 数据面就绪（空配置时为 503，属正常）
curl http://127.0.0.1:9000/openapi.json # 在线 OpenAPI 文档
```

## 1. 鉴权准备

控制面鉴权由环境变量 `CONROGATE_CONTROL_AUTH_TOKEN` 控制，支持逗号分隔多个 `operator:secret:role`：

```bash
export CONROGATE_CONTROL_AUTH_TOKEN='admin:my-admin-secret:admin,ops:my-ops-secret:operator,viewer:my-viewer-secret:viewer'
```

- token 为空 → **无鉴权模式**（所有受保护接口直接放行，仅限本地调试）。
- 管理操作请使用 `admin` / `operator` 角色；`viewer` 只能查询。
- 所有请求带统一头：`Authorization: Bearer <完整token串>`。

本文档示例统一使用两个 shell 变量，方便直接复制执行：

```bash
BASE=http://127.0.0.1:9000/api/v1
TOKEN='admin:my-admin-secret:admin'     # 换用你自己的 token
AUTH="Authorization: Bearer $TOKEN"     # 请求头，用法：-H "$AUTH"
```

## 2. 核心概念：配置何时生效

| 数据面配置源（`CONROGATE_GATE_REFRESH_CONFIG_SOURCE`） | 生效方式 |
|------|----------|
| `db`（默认） | 数据面按轮询间隔（默认 5s）直读业务表，**改表即生效**，无需发布 |
| `http` | 数据面定时拉取控制面 API（读业务表），改表即生效 |
| `redis` | 数据面读 Redis 快照，**只有执行发布/回滚才会推新快照**，必须发布 |

> 无论哪种模式，都建议在变更后执行**发布**（`/configs/publish`）：生成不可变版本号，便于审计、diff 和回滚。Redis 模式下发布是必需步骤。

## 3. 快速上手：配置一条路由（标准流程）

演示数据（`cargo run -p conrogate-migrate -- --seed` 写入）已含示例路由；以下是手动配置完整流程。

### 3.1 创建上游

```bash
curl -s -X POST "$BASE/upstreams" -H "$AUTH" -H 'Content-Type: application/json' -d '{
  "name": "product-api",
  "algorithm": "round_robin",
  "nodes": [
    {"address": "10.0.0.31:8080", "weight": 5},
    {"address": "10.0.0.32:8080", "weight": 3}
  ]
}'
```

`algorithm` 取值：`round_robin` / `weighted_round_robin` / `least_connections` / `consistent_hash`。返回 `data.id`（记为 `UPSTREAM_ID`）。

### 3.2 创建路由

```bash
curl -s -X POST "$BASE/routes" -H "$AUTH" -H 'Content-Type: application/json' -d '{
  "name": "product-route",
  "protocol": "http",
  "match_conditions": {
    "path": {"prefix": "/api/products"},
    "methods": ["GET", "POST"],
    "headers": [],
    "query_params": []
  },
  "upstream_id": 1,
  "priority": 10,
  "enabled": true
}'
```

`path` 三种匹配：`{"prefix": "/api"}` / `{"exact": "/health"}` / `{"regex": "^/v[0-9]+/"}`。
`protocol` 取值：`http` / `ws`（WebSocket）/ `tcp`（TCP 隧道）。返回 `data.id`（记为 `ROUTE_ID`）。

### 3.3 绑定插件（可选）

路由绑定插件（JWT 鉴权 / CORS 跨域 / 访问日志）的原理、配置与用法见对应插件文档（内置模块文档，`cargo doc -p conrogate-core` 可查）：

- `conrogate-core/src/plugins/auth/mod.rs` — 鉴权插件 `auth`
- `conrogate-core/src/plugins/cors/mod.rs` — 跨域插件 `cors`
- `conrogate-core/src/plugins/log/mod.rs` — 日志插件 `log`

绑定后需发布配置（见 3.4）才生效。

### 3.4 发布配置版本

```bash
curl -s -X POST "$BASE/configs/publish?remark=add%20product%20route" -H "$AUTH"
```

返回当前版本号（记为 `VERSION`）。`base_version` 参数默认 `0`（全量快照），后续发布可指定上次版本号。

### 3.5 验证流量

```bash
curl -i http://127.0.0.1:8080/api/products/123   # 应被 3.2 的路由匹配并转发到上游
```

## 4. 路由与上游日常操作

```bash
# 列表 / 详情
curl -s "$BASE/routes?page=1&page_size=20" -H "$AUTH"
curl -s "$BASE/upstreams" -H "$AUTH"
curl -s "$BASE/routes/$ROUTE_ID" -H "$AUTH"

# 更新（PUT 整体覆盖；PATCH 仅改传入字段）
curl -s -X PATCH "$BASE/routes/$ROUTE_ID" -H "$AUTH" -H 'Content-Type: application/json' \
  -d '{"enabled": false}'
curl -s -X PUT "$BASE/upstreams/$UPSTREAM_ID" -H "$AUTH" -H 'Content-Type: application/json' \
  -d '{"id": 1, "name": "product-api", "algorithm": "least_connections", "nodes": [{"address": "10.0.0.31:8080"}]}'

# 删除（软删除）
curl -s -X DELETE "$BASE/routes/$ROUTE_ID" -H "$AUTH"
curl -s -X DELETE "$BASE/upstreams/$UPSTREAM_ID" -H "$AUTH"
```

> 注意：路由被删除后其插件绑定一并失效；变更后如需留档请重新发布（见 §5）。

## 5. 配置版本管理

```bash
# 版本历史
curl -s "$BASE/configs/versions" -H "$AUTH"

# 两个版本差异
curl -s "$BASE/configs/diff?from=1&to=2" -H "$AUTH"

# 回滚到指定版本（会回写业务表 + 生成新版本号，非覆盖式）
curl -s -X POST "$BASE/configs/versions/1/rollback" -H "$AUTH"
```

## 6. 观测与排查

```bash
# 网关节点心跳状态（分离模式：数据面每 30s 上报，last_seen 持久化）
curl -s "$BASE/nodes" -H "$AUTH"

# 指标（最近 10 分钟）
curl -s "$BASE/metrics?range_min=10" -H "$AUTH"
curl -s "$BASE/metrics/overview?range_min=10" -H "$AUTH"

# 洞察
curl -s "$BASE/insights/qps?range_min=10" -H "$AUTH"
curl -s "$BASE/insights/latency?range_min=10" -H "$AUTH"
curl -s "$BASE/insights/status-codes?range_min=10" -H "$AUTH"
curl -s "$BASE/insights/top-routes?range_min=10" -H "$AUTH"

# 事件 / 审计日志
curl -s "$BASE/insights/events?event_type=error" -H "$AUTH"
curl -s "$BASE/audit-logs?action=publish" -H "$AUTH"
```

## 7. 运维注意

- **鉴权**：`CONROGATE_CONTROL_AUTH_TOKEN` 为空即无鉴权，严禁用于生产。token 串若不含 `:role` 段，角色回退为 `viewer`，写操作会被拒绝（错误码 `10003`）。
- **Redis 模式故障降级**：Redis 写入失败会自动重试（3 次 × 200ms），仍失败则删除版本键使数据面降级直连 DB 轮询；日志中 `config cache invalidate failed` 表示降级已触发。
- **健康检查**：`/health`（进程存活）、`/healthz`（存活探针）、`/readyz`（就绪探针，空配置时返回 503 属正常）。
- **回滚语义**：回滚是生成新版本号并回写业务表，不会删除历史版本；如需二次回滚再对旧版本执行即可。
- **限流/熔断**：集群模式需 `CONROGATE_GATE_RATE_LIMIT_MODE=cluster` / `CONROGATE_GATE_BREAKER_MODE`（或对应 Redis URL 已配置），否则按单机模式运行。

## 8. 常见问题（FAQ）

| 现象 | 排查方向 |
|------|----------|
| 配置改了但数据面不生效 | 确认 `CONROGATE_GATE_REFRESH_CONFIG_SOURCE`：`redis` 模式必须先执行 `/configs/publish` |
| 请求 403 / `10003 无权限` | 角色不足；token 是否严格 `operator:secret:role` 三段（无冒号 → viewer） |
| 请求 401 / `10002 unauthorized` | token 未配置或不匹配；确认与 `CONROGATE_CONTROL_AUTH_TOKEN` 中某项完全一致 |
| 上游请求 5xx | 检查 `GET /api/v1/nodes` 心跳是否正常、上游 `address` 是否可达 |
| `/readyz` 返回 503 | 尚未加载任何路由（空配置）或配置热载异常，查看数据面日志 |
| 回滚后仍读到旧配置 | 数据面轮询有秒级延迟；Redis 模式确认快照已写回 |
