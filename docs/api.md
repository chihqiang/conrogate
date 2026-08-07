# Conrogate 控制面 API 接口文档

基础地址：`http://<host>:<port>`（默认端口 `9000`）

> **路由前缀**：管理路由与数据上报路由默认挂载在 `/api/v1` 前缀下（可由 `CONROGATE_CONTROL_LISTEN_API_PREFIX` 调整）。公开路由（`/health`、`/healthz`、`/readyz`、`/openapi.json`）位于根路径。本文档正文以去掉前缀的形式书写，实际调用时请拼接 `/api/v1`，例如 `GET /api/v1/routes`。

---

## 1. 统一响应结构

所有接口返回以下 JSON 信封格式：

```json
{
  "code": 0,
  "msg": "success",
  "data": <T | null>,
  "trace_id": "00000000000000000000000000000000"
}
```

| 字段       | 类型      | 说明                                                             |
|------------|-----------|------------------------------------------------------------------|
| `code`     | `i32`     | `0` 表示成功；非零为错误码（见[错误码](#错误码)）                 |
| `msg`      | `String`  | 人类可读的消息；成功时为 `"success"`，失败时为错误描述            |
| `data`     | `T/null`  | 响应数据载荷；错误或空成功时为 `null`                             |
| `trace_id` | `String`  | 32 位十六进制时间戳，用作请求追踪 ID，便于日志关联                |

---

## 2. 鉴权机制

所有**受保护路由**需要在请求头中携带：

```text
Authorization: Bearer <operator>:<secret>:<role>
```

其中 `<role>` 取值 `viewer` / `operator` / `admin`。服务端通过环境变量 `CONROGATE_CONTROL_AUTH_TOKEN` 配置一个或多个这样的完整 token（**逗号分隔**，例如 `alice:ak:admin,bob:bk:operator,carol:ck:viewer`），请求携带的 token 需与其中**任一**完全一致；角色由命中的 token 自身携带的 `<role>` 段决定。

| 角色        | 权限范围                                                       |
|-------------|----------------------------------------------------------------|
| `admin`     | 完全权限；可激活/停用/卸载插件                                 |
| `operator`  | 可创建/更新/删除路由、上游、插件绑定、发布配置版本              |
| `viewer`    | 只读（仅列表查询和详情查询）                                   |

注意事项：

- 未认证请求返回 HTTP `401`，错误码 `10002`；角色不足返回 HTTP `200` + 错误码 `10003`（无权限）。
- 若 token 不是严格 `operator:secret:role` 三段格式（例如纯随机密钥），则通过鉴权但**角色回退为 `viewer`**，写操作将被拒绝。
- 数据上报接口（心跳/指标/事件）不校验角色，仅要求 token 匹配。

---

## 3. 健康检查（公开接口，无需鉴权）

### `GET /health`

基础存活探针，用于负载均衡器或编排器判断进程是否存活。

**请求参数：** 无

**响应 `data`：**

```json
{ "status": "ok" }
```

---

### `GET /healthz`

存活探针（`/health` 的别名）。

**响应 `data`：**

```json
{ "status": "ok" }
```

---

### `GET /readyz`

就绪探针。检查数据库连通性以及是否已加载至少一条路由规则。

**请求参数：** 无

**响应（就绪）：** HTTP `200`

```json
{ "code": 0, "msg": "success", "data": { "status": "ok" }, "trace_id": "..." }
```

**响应（未就绪）：** HTTP `503`

```json
{
  "code": 50001,
  "msg": "not ready: no routes loaded",
  "data": null,
  "trace_id": "..."
}
```

| 失败原因                                     | 说明                              |
|----------------------------------------------|-----------------------------------|
| `not ready: <数据库错误信息>`                | 无法连接数据库                    |
| `not ready: no routes loaded`                | 数据库正常但未配置任何路由        |
| `not ready: route check failed: <错误信息>`  | 路由查询返回错误                  |

---

## 4. 路由管理

### `GET /routes`

分页查询路由列表。

**查询参数：**

| 参数        | 类型   | 是否必填 | 默认值 | 说明                         |
|-------------|--------|----------|--------|------------------------------|
| `page`      | `u32`  | 否       | `1`    | 页码（从 1 开始）            |
| `page_size` | `u32`  | 否       | `20`   | 每页条数                     |

**响应 `data`：**

```json
{
  "list": [ <RouteDto> ],
  "total": 42,
  "page": 1,
  "page_size": 20
}
```

**`RouteDto` 字段说明：**

| 字段                          | 类型                     | 说明                                                     |
|-------------------------------|--------------------------|----------------------------------------------------------|
| `id`                          | `u64`                    | 路由唯一标识（自动生成）                                  |
| `name`                        | `String`                 | 路由名称（人类可读）                                      |
| `protocol`                    | `ProtocolId`             | 协议类型：`"http"` / `"web_socket"` / `"tcp_tunnel"`      |
| `match_conditions`            | `RouteMatchConditions`   | 多维度匹配规则（见下表）                                  |
| `priority`                    | `i32`                    | 优先级，值越大越优先匹配；相同优先级按数据库顺序            |
| `upstream_id`                 | `u64/null`               | 匹配流量转发目标上游的 ID                                 |
| `host_header`                 | `String/null`            | 转发至上游时覆盖 Host 请求头的值                           |
| `allow_retry_non_idempotent`  | `bool`                   | 是否允许对非幂等请求（POST/PATCH/DELETE）进行重试          |
| `ws_strip_sensitive_headers`  | `bool`                   | WebSocket 代理转发时是否剥离 `authorization`/`cookie`/`x-api-key` 等敏感头 |
| `enabled`                     | `bool`                   | `false` 表示路由存在但匹配时被跳过                        |
| `created_at`                  | `DateTime<Utc>`          | 创建时间（ISO-8601）                                      |
| `updated_at`                  | `DateTime<Utc>`          | 最后修改时间                                              |

**`RouteMatchConditions` 字段说明（多条件 AND 关系）：**

| 字段           | 类型                    | 说明                                                                    |
|----------------|-------------------------|-------------------------------------------------------------------------|
| `path`         | `PathMatch`             | 必填。三选一：`{"Prefix": "/foo"}`（前缀匹配）、`{"Exact": "/foo"}`（精确匹配）、`{"Regex": ".*"}`（正则匹配） |
| `methods`      | `Vec<String>/null`      | 匹配的 HTTP 方法列表；`null` 表示匹配所有方法                           |
| `host`         | `String/null`           | 匹配 `Host` 请求头的值                                                   |
| `headers`      | `Vec<HeaderMatch>`      | 请求头匹配条件（所有条件必须同时满足）                                   |
| `query_params` | `Vec<QueryMatch>`       | 查询参数匹配条件                                                        |

**`HeaderMatch` / `QueryMatch` 字段说明：**

| 字段    | 类型        | 说明                                                              |
|---------|-------------|-------------------------------------------------------------------|
| `key`   | `String`    | 要匹配的请求头名或查询参数名                                      |
| `op`    | `MatchOp`   | 匹配操作符：`"exact"`（精确）/ `"prefix"`（前缀）/ `"regex"`（正则）/ `"not_empty"`（非空） |
| `value` | `String`    | 比较值（`not_empty` 时可省略）                                    |

**`ProtocolId` 枚举值：** `"http"` | `"web_socket"` | `"tcp_tunnel"`

---

### `GET /routes/:id`

根据 ID 查询单条路由详情。

**路径参数：**

| 参数 | 类型  | 说明     |
|------|-------|----------|
| `id` | `u64` | 路由 ID  |

**响应 `data`：** 单个 `RouteDto` 对象。

---

### `POST /routes`

创建新路由。

**权限要求：** `operator` 或 `admin`

**请求体（`CreateRouteDto`）：**

| 字段                          | 类型                     | 是否必填 | 默认值   | 说明                                   |
|-------------------------------|--------------------------|----------|----------|----------------------------------------|
| `name`                        | `String`                 | 是       | —        | 路由名称（须全局唯一）                  |
| `protocol`                    | `ProtocolId`             | 是       | —        | `"http"` / `"web_socket"` / `"tcp_tunnel"` |
| `match_conditions`            | `RouteMatchConditions`   | 是       | —        | 匹配规则（见上方说明）                  |
| `priority`                    | `i32`                    | 否       | `10`     | 值越大越优先匹配                        |
| `upstream_id`                 | `u64/null`               | 否       | `null`   | 转发目标上游 ID                        |
| `host_header`                 | `String/null`            | 否       | `null`   | 转发时覆盖 Host 请求头                  |
| `allow_retry_non_idempotent`  | `bool/null`              | 否       | `false`  | 允许重试非幂等请求                      |
| `ws_strip_sensitive_headers`  | `bool/null`              | 否       | `false`  | WS 代理剥离敏感头                       |
| `enabled`                     | `bool/null`              | 否       | `true`   | 路由是否立即生效                        |

**响应 `data`：** 新创建的 `RouteDto`（包含自动生成的 `id` 和时间戳）。

---

### `PUT /routes/:id`

全量更新路由（所有可变字段均需提供）。

**权限要求：** `operator` 或 `admin`

**路径参数：** `id: u64`

**请求体（`UpdateRouteDto`）：**

| 字段                          | 类型                     | 是否必填 | 说明                         |
|-------------------------------|--------------------------|----------|------------------------------|
| `id`                          | `u64`                    | 是       | 必须与路径参数一致            |
| `name`                        | `String`                 | 否       | 新路由名称                    |
| `match_conditions`            | `RouteMatchConditions`   | 否       | 新匹配规则                    |
| `priority`                    | `i32`                    | 否       | 新优先级                      |
| `upstream_id`                 | `u64/null`               | 否       | 新上游 ID（`null` 表示不转发）|
| `host_header`                 | `String/null`            | 否       | 新 Host 覆盖值                |
| `allow_retry_non_idempotent`  | `bool/null`              | 否       | 新重试标志                    |
| `ws_strip_sensitive_headers`  | `bool/null`              | 否       | 新 WS 剥离敏感头标志          |
| `enabled`                     | `bool/null`              | 否       | 新启用状态                    |

**响应 `data`：** 更新后的 `RouteDto`。

---

### `PATCH /routes/:id`

局部更新路由。仅提供需要修改的字段，未提供的字段保持不变。

**权限要求：** `operator` 或 `admin`

路径和请求体与 `PUT /routes/:id` 相同，请求体中的 `id` 会从路径参数自动覆盖。

**响应 `data`：** 更新后的 `RouteDto`。

---

### `DELETE /routes/:id`

永久删除路由。

**权限要求：** `operator` 或 `admin`

**路径参数：** `id: u64`

**响应 `data`：** `null`（空成功）

**错误码：** `10004`（资源不存在）

---

## 5. 上游管理

### `GET /upstreams`

分页查询上游列表。

**查询参数：** `page`、`page_size`（与 `/routes` 相同）

**响应 `data`：**

```json
{
  "list": [ <UpstreamDto> ],
  "total": 12,
  "page": 1,
  "page_size": 20
}
```

**`UpstreamDto` 字段说明：**

| 字段           | 类型                    | 说明                                                 |
|----------------|-------------------------|------------------------------------------------------|
| `id`           | `u64`                   | 上游唯一标识                                          |
| `name`         | `String`                | 上游名称（人类可读）                                  |
| `algorithm`    | `BalancerAlgorithm`     | 负载均衡算法                                          |
| `retry_enabled`| `bool`                  | 是否对失败请求自动重试                                |
| `nodes`        | `Vec<UpstreamNodeDto>`  | 属于该上游的后端节点列表                              |
| `created_at`   | `DateTime<Utc>`         | 创建时间                                              |
| `updated_at`   | `DateTime<Utc>`         | 最后修改时间                                          |

**`UpstreamNodeDto` 字段说明：**

| 字段          | 类型     | 说明                                                       |
|---------------|----------|------------------------------------------------------------|
| `id`          | `u64`    | 节点唯一标识（自动生成）                                    |
| `upstream_id` | `u64`    | 所属上游 ID                                                |
| `address`     | `String` | 后端地址，格式 `host:port`，例如 `127.0.0.1:9090`          |
| `weight`      | `i32`    | 相对权重（`weighted_round_robin` 算法使用）                 |
| `enabled`     | `bool`   | `false` 表示节点被禁用，负载均衡时跳过                      |

**`BalancerAlgorithm` 枚举值说明：**

| 值                      | 说明                                                     |
|-------------------------|----------------------------------------------------------|
| `round_robin`           | 轮询：在所有节点间均匀轮转                                |
| `weighted_round_robin`  | 加权轮询：按权重比例分配流量                              |
| `least_connections`     | 最少连接数：将请求路由到当前活跃连接最少的节点             |
| `consistent_hash`       | 一致性哈希：基于请求键的哈希值实现会话粘滞                |

---

### `GET /upstreams/:id`

根据 ID 查询单个上游详情。

**响应 `data`：** 单个 `UpstreamDto` 对象。

---

### `POST /upstreams`

创建新上游及其后端节点。

**权限要求：** `operator` 或 `admin`

**请求体（`CreateUpstreamDto`）：**

| 字段           | 类型                            | 是否必填 | 默认值   | 说明                         |
|----------------|---------------------------------|----------|----------|------------------------------|
| `name`         | `String`                        | 是       | —        | 上游名称（须全局唯一）        |
| `algorithm`    | `BalancerAlgorithm`             | 是       | —        | 负载均衡算法                  |
| `retry_enabled`| `bool/null`                     | 否       | `false`  | 是否启用自动重试              |
| `nodes`        | `Vec<CreateUpstreamNodeDto>`    | 是       | —        | 至少提供一个节点              |

**`CreateUpstreamNodeDto` 字段说明：**

| 字段      | 类型         | 是否必填 | 默认值   | 说明                           |
|-----------|--------------|----------|----------|--------------------------------|
| `address` | `String`     | 是       | —        | 后端地址 `host:port`           |
| `weight`  | `i32/null`   | 否       | `1`      | 相对权重                        |
| `enabled` | `bool/null`  | 否       | `true`   | 节点是否启用                    |

**响应 `data`：** 新创建的 `UpstreamDto`（包含自动生成的 `id`、节点列表和时间戳）。

---

### `PUT /upstreams/:id`

全量更新上游。提供 `nodes` 时会替换整个节点列表。

**权限要求：** `operator` 或 `admin`

**请求体（`UpdateUpstreamDto`）：**

| 字段           | 类型                             | 是否必填 | 说明                          |
|----------------|----------------------------------|----------|-------------------------------|
| `id`           | `u64`                            | 是       | 必须与路径参数一致             |
| `name`         | `String`                         | 否       | 新上游名称                     |
| `algorithm`    | `BalancerAlgorithm`              | 否       | 新负载均衡算法                 |
| `retry_enabled`| `bool/null`                      | 否       | 新重试标志                     |
| `nodes`        | `Vec<CreateUpstreamNodeDto>/null`| 否       | 替换完整节点列表（`null` 保留）|

**响应 `data`：** 更新后的 `UpstreamDto`。

---

### `PATCH /upstreams/:id`

局部更新上游。未提供的字段保持不变。

**权限要求：** `operator` 或 `admin`

请求体与 `PUT` 相同，`id` 从路径参数自动覆盖。

**响应 `data`：** 更新后的 `UpstreamDto`。

---

### `DELETE /upstreams/:id`

永久删除上游及其所有节点（级联删除）。

**权限要求：** `operator` 或 `admin`

**响应 `data`：** `null`

---

## 6. 配置版本管理

### `POST /configs/publish`

将当前配置（路由 + 上游 + 插件绑定）快照为新版本。

**权限要求：** `operator` 或 `admin`

**查询参数：**

| 参数           | 类型     | 是否必填 | 默认值 | 说明                                              |
|----------------|----------|----------|--------|---------------------------------------------------|
| `base_version` | `u64`    | 否       | `0`    | 基准版本号；`0` 表示发布完整快照                   |
| `remark`       | `String` | 否       | —      | 备注（如 "release v2.1"）                          |

**响应 `data`：**

```json
{
  "version": 3,
  "base_version": 2,
  "publish_type": "publish",
  "content_hash": "a1b2c3d4e5f6...",
  "created_by": "admin",
  "remark": "release v2.1",
  "applied_count": 0,
  "created_at": "2026-01-20T15:00:00Z"
}
```

**`ConfigVersionDto` 字段说明：**

| 字段           | 类型             | 说明                                              |
|----------------|------------------|---------------------------------------------------|
| `version`      | `u64`            | 单调递增的版本号                                   |
| `base_version` | `u64`            | 基于哪个版本发布                                   |
| `publish_type` | `PublishType`    | `"publish"`（正常发布）/ `"rollback"`（回滚）      |
| `content_hash` | `String`         | 配置快照的 SHA-256 哈希值                          |
| `created_by`   | `String/null`    | 执行发布的操作人                                   |
| `remark`       | `String/null`    | 备注信息                                           |
| `applied_count`| `u32`            | 已应用此版本的网关节点数量                         |
| `created_at`   | `DateTime<Utc>`  | 发布时间                                           |

---

### `GET /configs/versions`

分页查询配置版本历史（最新版本排在前面）。

**查询参数：** `page`、`page_size`

**响应 `data`：** `PaginatedResult<ConfigVersionDto>`

---

### `POST /configs/versions/:version/rollback`

回滚到指定历史版本。系统会基于该版本的快照创建一个新版本（`publish_type` 为 `"rollback"`）。

**权限要求：** `operator` 或 `admin`

**路径参数：** `version: u64` — 要恢复的目标版本号

**响应 `data`：** 新的 `ConfigVersionDto`，其中 `publish_type` 为 `"rollback"`

---

### `GET /configs/diff`

对比两个配置版本的差异，展示新增、修改和删除的资源。

**查询参数：**

| 参数   | 类型  | 是否必填 | 说明             |
|--------|-------|----------|------------------|
| `from` | `u64` | 是       | 源版本号          |
| `to`   | `u64` | 是       | 目标版本号        |

**响应 `data`：**

```json
{
  "added":    ["upstream #5", "route #12"],
  "modified": ["route #3"],
  "removed":  ["route #8", "upstream #2"]
}
```

每条记录为人类可读的资源变更描述。

---

## 7. 指标与洞察分析

### `GET /metrics`

查询指定时间范围内的原始指标行数据。

**查询参数（`MetricQuery`）：**

| 参数        | 类型     | 是否必填 | 说明                                   |
|-------------|----------|----------|----------------------------------------|
| `range_min` | `u32`    | 是       | 时间范围（分钟），如 `5`、`60`、`1440` |
| `route_id`  | `u64`    | 否       | 按路由 ID 过滤                         |
| `gate_id`   | `String` | 否       | 按网关实例 ID 过滤                     |

**响应 `data`：** `MetricRow` 数组

| 字段              | 类型            | 说明                                                       |
|-------------------|-----------------|------------------------------------------------------------|
| `ts`              | `DateTime<Utc>` | 时间桶时间戳                                                |
| `bucket_sec`      | `u32`           | 时间桶粒度（秒），如 `10`、`60`                             |
| `route_id`        | `u64/null`      | 该指标所属路由（`null` 表示全路由聚合）                     |
| `gate_id`         | `String`        | 上报此指标的网关实例 ID                                    |
| `qps`             | `u32`           | 每秒请求数（时间桶内的平均值）                              |
| `total_requests`  | `u64`           | 时间桶内的总请求数                                          |
| `avg_latency_ms`  | `f64`           | 平均响应延迟（毫秒）                                       |
| `p50_ms`          | `u32`           | P50 延迟（毫秒）                                            |
| `p90_ms`          | `u32`           | P90 延迟（毫秒）                                            |
| `p99_ms`          | `u32`           | P99 延迟（毫秒）                                            |
| `status_2xx`      | `u64`           | 2xx 响应计数                                                |
| `status_3xx`      | `u64`           | 3xx 响应计数                                                |
| `status_4xx`      | `u64`           | 4xx 响应计数                                                |
| `status_5xx`      | `u64`           | 5xx 响应计数                                                |
| `sessions`        | `u64`           | 活跃会话/连接数（WebSocket/TCP 隧道）                       |
| `bytes_in`        | `u64`           | 从客户端接收的总字节数                                      |
| `bytes_out`       | `u64`           | 发送到客户端的总字节数                                      |

---

### `GET /metrics/overview`

聚合指标概览，提供全局视角的流量健康状况。

**查询参数：**

| 参数        | 类型  | 是否必填 | 默认值 | 说明                   |
|-------------|-------|----------|--------|------------------------|
| `range_min` | `u32` | 否       | `5`    | 时间范围（分钟）        |

**响应 `data`：**

```json
{
  "total_qps": 1234.5,
  "avg_latency_ms": 45.2,
  "error_rate": 0.02
}
```

| 字段             | 类型   | 说明                                             |
|------------------|--------|--------------------------------------------------|
| `total_qps`      | `f64`  | 全路由每秒请求数总和                              |
| `avg_latency_ms` | `f64`  | 加权平均响应延迟（毫秒）                          |
| `error_rate`     | `f64`  | 错误率 = (4xx + 5xx) / 总请求数，取值范围 0.0~1.0 |

---

### `GET /insights/overview`

与 `GET /metrics/overview` 功能相同（别名接口）。

---

### `GET /insights/qps`

获取 QPS 时序数据，用于绘制时序图表。

**查询参数：** `range_min: u32`（否，默认 `5`）

**响应 `data`：**

```json
{
  "series": [
    { "ts": "2026-01-20T15:00:00Z", "qps": 1200 },
    { "ts": "2026-01-20T15:00:10Z", "qps": 1350 }
  ]
}
```

每条记录为一个 10 秒时间桶及其平均 QPS。

---

### `GET /insights/latency`

获取延迟百分位数汇总。

**查询参数：** `range_min: u32`（否，默认 `5`）

**响应 `data`：**

```json
{
  "avg_ms": 45.2,
  "p50_ms": 32,
  "p90_ms": 120,
  "p99_ms": 250
}
```

| 字段      | 类型   | 说明                   |
|-----------|--------|------------------------|
| `avg_ms`  | `f64`  | 平均延迟（毫秒）        |
| `p50_ms`  | `u32`  | P50 延迟（毫秒）        |
| `p90_ms`  | `u32`  | P90 延迟（毫秒）        |
| `p99_ms`  | `u32`  | P99 延迟（毫秒）        |

---

### `GET /insights/status-codes`

获取状态码分布汇总。

**查询参数：** `range_min: u32`

**响应 `data`：**

```json
{
  "2xx": 150000,
  "3xx": 500,
  "4xx": 1200,
  "5xx": 30
}
```

---

### `GET /insights/top-routes`

获取按请求量排序的热门路由排行。

**查询参数：** `range_min: u32`

**响应 `data`：**

```json
{
  "top_routes": [
    { "route_id": 3, "total_requests": 85000 },
    { "route_id": 1, "total_requests": 42000 }
  ]
}
```

按 `total_requests` 降序排列。

---

## 8. 事件查询

### `GET /insights/events`

查询网关事件（错误、插件终止等）。

**查询参数（`EventQuery` + `PaginationQuery`）：**

| 参数         | 类型       | 是否必填 | 默认值 | 说明                       |
|--------------|------------|----------|--------|----------------------------|
| `event_type` | `String`   | 否       | —      | 按事件类型过滤              |
| `route_id`   | `u64`      | 否       | —      | 按路由 ID 过滤              |
| `ts_from`    | `DateTime` | 否       | —      | 起始时间（ISO-8601）        |
| `ts_to`      | `DateTime` | 否       | —      | 截止时间（ISO-8601）        |
| `page`       | `u32`      | 否       | `1`    | 页码                        |
| `page_size`  | `u32`      | 否       | `20`   | 每页条数                    |

**响应 `data`：** `PaginatedResult<EventRow>`

| 字段          | 类型                | 说明                                                       |
|---------------|---------------------|------------------------------------------------------------|
| `ts`          | `DateTime<Utc>`     | 事件发生时间                                                |
| `event_type`  | `String`            | 事件类别，如 `"upstream_timeout"`、`"plugin_terminate"`     |
| `route_id`    | `u64/null`          | 关联路由 ID（如适用）                                       |
| `upstream_id` | `u64/null`          | 关联上游 ID（如适用）                                       |
| `trace_id`    | `String/null`       | 请求追踪 ID，用于日志关联                                   |
| `detail`      | `serde_json::Value` | 结构化事件详情（字段结构因事件类型而异）                     |

---

## 9. 审计日志

### `GET /audit-logs`

查询管理操作审计记录（谁在什么时间做了什么操作）。

**查询参数（`AuditLogQuery` + `PaginationQuery`）：**

| 参数       | 类型       | 是否必填 | 默认值 | 说明                                       |
|------------|------------|----------|--------|--------------------------------------------|
| `operator` | `String`   | 否       | —      | 按操作人过滤                                |
| `action`   | `String`   | 否       | —      | 按动作过滤，如 `"create_route"`、`"delete_upstream"` |
| `resource` | `String`   | 否       | —      | 按资源类型过滤                              |
| `ts_from`  | `DateTime` | 否       | —      | 起始时间                                    |
| `ts_to`    | `DateTime` | 否       | —      | 截止时间                                    |
| `page`     | `u32`      | 否       | `1`    | 页码                                        |
| `page_size`| `u32`      | 否       | `20`   | 每页条数                                    |

**响应 `data`：** `PaginatedResult<AuditLogRow>`

| 字段          | 类型                | 说明                                                     |
|---------------|---------------------|----------------------------------------------------------|
| `ts`          | `DateTime<Utc>`     | 操作执行时间                                              |
| `operator`    | `String/null`       | 操作人身份（来自鉴权令牌）                                |
| `action`      | `String`            | 执行的动作，如 `"create_route"`                           |
| `resource`    | `String`            | 资源类型，如 `"route"`、`"upstream"`                      |
| `resource_id` | `u64/null`          | 被操作资源的 ID                                           |
| `detail`      | `serde_json::Value` | 变更快照（包含变更前后数据）                               |
| `trace_id`    | `String/null`       | 请求追踪 ID                                               |

---

## 10. 节点管理

### `GET /nodes`

查询所有已注册网关节点及其已应用的配置版本。

**权限要求：** 所有角色

**响应 `data`：** `NodeApplicationRow` 数组

| 字段         | 类型            | 说明                                       |
|--------------|-----------------|--------------------------------------------|
| `gate_id`    | `String`        | 网关实例唯一标识                            |
| `version`    | `u64`           | 该网关已应用的配置版本号                    |
| `applied_at` | `DateTime<Utc>` | 版本应用时间                                |
| `updated_at` | `DateTime<Utc>` | 最后心跳/更新时间                           |

合并模式下此列表通常为空；分离模式下，各 `conrogate-gate` 数据面实例定期上报心跳后会填充。

---

## 11. 数据上报（数据面 → 控制面）

以下接口由 `conrogate-gate` 数据面实例调用，用于上报遥测数据。属于**受保护路由**（需要共享鉴权令牌）。

### `POST /reports/heartbeat`

上报网关存活状态及当前配置版本。

**请求体（`Heartbeat`）：**

| 字段        | 类型            | 说明                           |
|-------------|-----------------|--------------------------------|
| `gate_id`   | `String`        | 网关实例唯一标识                |
| `version`   | `u64`           | 当前已应用的配置版本号          |
| `timestamp` | `DateTime<Utc>` | 心跳时间戳                      |

**响应 `data`：** `null`

---

### `POST /reports/metrics`

上报网关聚合后的指标批次数据。

**请求体（`MetricsBatch`）：**

| 字段           | 类型              | 说明                                       |
|----------------|-------------------|--------------------------------------------|
| `gate_id`      | `String`          | 上报的网关实例 ID                           |
| `trace_id`     | `String`          | 批次级追踪 ID                               |
| `window_start` | `DateTime<Utc>`   | 指标窗口起始时间                            |
| `window_end`   | `DateTime<Utc>`   | 指标窗口结束时间                            |
| `bucket_sec`   | `u32`             | 时间桶粒度（秒）                            |
| `metrics`      | `Vec<MetricRow>`  | 指标行数组（结构同 §7 指标与洞察）                 |

**响应 `data`：** `null`

---

### `POST /reports/events`

上报网关事件批次数据。

**请求体（`EventsBatch`）：**

| 字段       | 类型             | 说明                                   |
|------------|------------------|----------------------------------------|
| `gate_id`  | `String`         | 上报的网关实例 ID                       |
| `trace_id` | `String`         | 批次级追踪 ID                           |
| `events`   | `Vec<EventRow>`  | 事件行数组（结构同 §8 事件查询）            |

**响应 `data`：** `null`

---

## 12. OpenAPI 规范

### `GET /openapi.json`

返回自动生成的 OpenAPI 3.x JSON 规范文档（包含所有端点定义）。

**响应：** `application/json` — 完整 OpenAPI 文档（不使用统一信封格式）

---

## 错误码

> 控制面统一信封的 HTTP 状态码：`401`（未认证）/ `500`（内部错误）/ 其余业务错误均 `200`（通过 `code` 区分）。数据面（`4000x` 网关错误）由数据面按其自身映射返回（见下表）。

| 错误码   | HTTP 状态码          | 说明                             |
|----------|----------------------|----------------------------------|
| `0`      | `200`                | 成功                             |
| `10001`  | `200`                | 请求参数非法                     |
| `10002`  | `401`                | 未认证（缺少或无效的令牌）       |
| `10003`  | `200`                | 权限不足（角色不满足）           |
| `10004`  | `200`                | 资源不存在                       |
| `10005`  | `200`                | 资源冲突（重复创建）             |
| `10006`  | `200`                | 请求过于频繁（限流）             |
| `10007`  | `200`                | 请求体过大                       |
| `20001`  | `200`                | 路由不存在                       |
| `20002`  | `200`                | 上游不存在                       |
| `20003`  | `200`                | 插件配置非法                     |
| `20004`  | `200`                | 插件未找到                       |
| `20005`  | `200`                | 插件运行时错误                   |
| `20006`  | `200`                | 配置非法                         |
| `20007`  | `200`                | 配置并发冲突                     |
| `30001`  | `500`                | 数据库内部错误                   |
| `30002`  | `500`                | 数据映射错误                     |
| `30003`  | `500`                | 数据库迁移错误                   |
| `40001`  | `500`                | 网络内部错误                     |
| `40002`  | `502`                | 上游超时（数据面映射为 502/40006）|
| `40003`  | `502`                | 上游连接失败（数据面映射为 502/40006）|
| `40004`  | `502`                | 上游返回异常响应（数据面映射为 502/40006）|
| `40005`  | `200`                | 协议不支持                       |
| `40006`  | `502`                | 网关内部错误                     |
| `40007`  | `503`                | 熔断器打开                       |
| `40008`  | `429`                | 被限流器拒绝                     |
| `40009`  | `502`                | 重试耗尽                         |
| `50001`  | `500` / `503`        | 配置加载失败（服务端）/ 服务未就绪（就绪探针）|
| `50002`  | `500`                | 初始化失败                       |
| `59999`  | `500`                | 内部错误（兜底）                 |
