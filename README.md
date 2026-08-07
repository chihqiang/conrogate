# Conrogate

Conrogate 轻量级微服务网关，内置配置中心，支持动态路由、负载均衡与流量管控；提供插件扩展机制，采用编译集成模式。

## 特性

- **动态路由**：前缀/精确/正则路径匹配 + Host/Header/Query 多维匹配
- **负载均衡**：轮询 / 加权轮询 / 最少连接 / 一致性哈希
- **流量治理**：限流（令牌桶）+ 熔断 + 重试 + 超时
- **协议支持**：HTTP/1.1 + HTTP/2 + WebSocket + TCP 隧道
- **插件系统**：静态编译插件；内置 CORS / Auth / Header-Rewrite 插件
- **控制面**：REST API 管理路由 / 上游 / 插件 / 配置版本；OpenAPI 文档
- **配置热载**：DB 轮询 / Redis Pub/Sub 推送 / HTTP 拉取，秒级生效
- **部署灵活**：合并模式（单进程双端口）/ 分离模式（gate × N + control × 1~2）

## 快速开始

```bash
# 1. 启动 MySQL
docker run -d --name conrogate-mysql \
  -e MYSQL_ROOT_PASSWORD=rootpass \
  -e MYSQL_DATABASE=conrogate \
  -e MYSQL_USER=conrogate \
  -e MYSQL_PASSWORD=conrogatepass \
  -p 3306:3306 mysql:8.0

# 2. 迁移（--seed 写入演示路由，供步骤 4 验证）
CONROGATE_DB_URL='mysql://conrogate:conrogatepass@127.0.0.1:3306/conrogate' \
  cargo run -p conrogate-migrate -- --seed

# 3. 合并模式启动（8080 数据面 + 9000 控制面）
CONROGATE_DB_URL='mysql://conrogate:conrogatepass@127.0.0.1:3306/conrogate' \
CONROGATE_LOG_OUTPUT_FILE_ENABLED=false \
CONROGATE_CONTROL_AUTH_TOKEN=admin:dev-token:admin \
  cargo run -p conrogate

# 4. 验证
curl http://localhost:9000/health         # 控制面（公开）
curl http://localhost:9000/healthz        # 存活探针
curl http://localhost:8080/demo/hello     # 数据面转发
```

> 也可以使用一键脚本启动：`./scripts/dev-up.sh`（自动拉起 PG + Redis → 迁移 → 合并模式启动）。
> 接入示例：`./scripts/test-httpbin-svc.sh`（把 httpbin.org 注册为上游并转发验证）。
> WebSocket 隧道测试：`./scripts/test-ws-svc.sh`（`scripts/ws.php` 用 Swoole 起本地 WS echo 上游，注册后经网关回显校验，`--cleanup` 清理）。
> SSE 流式测试：`./scripts/test-sse-svc.sh`（`scripts/sse.php` 起本地 SSE 上游，校验网关流式透传：正文与直连一致且首字节远早于流结束，`--cleanup` 清理）。

## 三种部署模式

| 模式 | 二进制 | 端口 | 适用场景 |
|------|--------|------|----------|
| 合并模式 | `conrogate` | 8080 + 9000 | 开发 / 小规模生产 |
| 分离模式 | `conrogate-gate` × N + `conrogate-control` × 1~2 | 8080 / 9000 | 生产 / 大规模 |
| 迁移工具 | `conrogate-migrate` | — | 部署前置执行 |

详细部署指南见 → [`docs/deployment.md`](docs/deployment.md)

## 配置

所有配置通过环境变量加载（支持 `.env` 文件）。环境模板：`.env.example`（本地测试，SQLite）/ `.env.prod.example`（生产，PostgreSQL/MySQL）。

**最小配置：**

```bash
CONROGATE_DB_URL='mysql://conrogate:conrogatepass@127.0.0.1:3306/conrogate'
CONROGATE_LOG_OUTPUT_FILE_ENABLED=false
```

**生产推荐配置：**

```bash
CONROGATE_DB_URL='mysql://conrogate:conrogatepass@127.0.0.1:3306/conrogate'
CONROGATE_DB_READ_URL='mysql://readonly:ro@slave:3306/conrogate'
CONROGATE_GATE_PORT=8080
CONROGATE_CONTROL_LISTEN_PORT=9000
CONROGATE_CONTROL_AUTH_TOKEN='admin:admin-secret:admin,ops:ops-secret:operator,guest:guest-secret:viewer'
CONROGATE_GATE_CONFIG_CACHE_REDIS_URL='redis://127.0.0.1:6379'
CONROGATE_GATE_RATE_LIMIT_ENABLED=true
CONROGATE_GATE_BREAKER_ENABLED=true
CONROGATE_LOG_LEVEL=warn
CONROGATE_LOG_OUTPUT_FILE_ENABLED=false
```

完整变量参考 → [`docs/env.md`](docs/env.md)

## API

控制面 REST API 文档：`GET /openapi.json`

> 管理路由与数据上报路由统一挂载在 `api_prefix` 前缀（默认 `/api/v1`，可用 `CONROGATE_CONTROL_LISTEN_API_PREFIX` 调整），gate 侧通过 `CONROGATE_GATE_REFRESH_CONTROL_API_PREFIX` 对齐。

**公开路由（无需鉴权）：**

| 端点 | 方法 | 说明 |
|------|------|------|
| `/health` | GET | 健康检查 |
| `/healthz` | GET | 存活探针 |
| `/readyz` | GET | 就绪探针 |
| `/openapi.json` | GET | OpenAPI 文档 |

**管理路由（需 `Authorization: Bearer <token>`，前缀 `/api/v1`）：**

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/v1/routes` | GET/POST | 路由列表 / 创建 |
| `/api/v1/routes/:id` | GET/PUT/PATCH/DELETE | 路由详情 / 更新 / 删除 |
| `/api/v1/upstreams` | GET/POST | 上游列表 / 创建 |
| `/api/v1/upstreams/:id` | GET/PUT/PATCH/DELETE | 上游详情 / 更新 / 删除 |
| `/api/v1/routes/:id/plugins` | GET/POST | 插件绑定列表 / 绑定插件 |
| `/api/v1/routes/:id/plugins/:plugin_name` | PUT/DELETE | 更新绑定 / 解绑插件 |
| `/api/v1/configs/publish` | POST | 发布配置版本 |
| `/api/v1/configs/versions` | GET | 版本历史列表 |
| `/api/v1/configs/versions/:v/rollback` | POST | 回滚到指定版本 |
| `/api/v1/configs/diff?from=&to=` | GET | 两个版本 Diff |
| `/api/v1/metrics` | GET | 指标查询 |
| `/api/v1/metrics/overview` | GET | 指标概览 |
| `/api/v1/insights/qps` | GET | QPS 时序 |
| `/api/v1/insights/latency` | GET | 延迟分布（p50/p90/p99） |
| `/api/v1/insights/status-codes` | GET | 状态码分布 |
| `/api/v1/insights/top-routes` | GET | 热门路由 TOP 10 |
| `/api/v1/insights/events` | GET | 事件查询 |
| `/api/v1/audit-logs` | GET | 操作审计日志 |
| `/api/v1/nodes` | GET | 网关节点列表 |
| `/api/v1/plugins` | GET | 已安装插件列表 |
| `/api/v1/plugins/:name/activate` | POST | 启用插件 |
| `/api/v1/plugins/:name/disable` | POST | 禁用插件 |
| `/api/v1/plugins/:name` | DELETE | 卸载插件 |

**数据上报路由（gate → control，前缀 `/api/v1`）：**

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/v1/reports/heartbeat` | POST | 节点心跳上报（30s） |
| `/api/v1/reports/metrics` | POST | 指标批量上报 |
| `/api/v1/reports/events` | POST | 事件批量上报 |

完整 API 文档 → [`docs/api.md`](docs/api.md)

## 架构

详细协调架构见 → [`docs/architecture.md`](docs/architecture.md)
插件（CORS / Auth / Header-Rewrite）使用说明见 → [`docs/plugins.md`](docs/plugins.md)

```text
控制面 (Control Plane :9000)
  REST API → 审计日志 → 配置版本发布 → Redis 快照推送
                                      ↓
数据面 (Data Plane :8080) ←── 配置热加载 ←── DB 轮询 / Redis 快照 / HTTP 拉取
  路由匹配 → 插件链 → 限流/熔断 → 负载均衡 → 转发
       ↓ 遥测采集
  ── mpsc ──► 指标聚合 / 事件批量落库（合并模式）
  ── HTTP ──► /reports/* 端点（分离模式）
```

## 开发

```bash

cargo check --workspace              # 编译检查
cargo test --workspace               # 运行测试
cargo clippy --workspace             # 代码质量
cargo fmt --all                      # 统一代码风格
cargo run -p conrogate-migrate       # 手动执行数据库迁移
cargo run -p conrogate               # 合并模式（数据面 8080 + 控制面 9000）
cargo run -p conrogate-control       # 分离模式：控制面（9000）
cargo run -p conrogate-gate          # 分离模式：数据面（8080）
```

### Docker

```bash
docker build -t ghcr.io/chihqiang/conrogate:latest .                 # 构建镜像
docker compose -f docker-compose.deps.yml up -d    # 起依赖（PG + Redis）
```

**合并模式运行（SQLite + 数据卷持久化）：**

```bash
# ① 先迁移建表（数据落在命名卷 conrogate-data，挂载到 /data）
docker run --rm \
  -v conrogate-data:/data \
  -e CONROGATE_DB_URL=sqlite:///data/conrogate.db \
  ghcr.io/chihqiang/conrogate:latest conrogate-migrate

# ② 启动（8080 数据面 + 9000 控制面）
docker run -d --name conrogate \
  -v conrogate-data:/data \
  -e CONROGATE_DB_URL=sqlite:///data/conrogate.db \
  -e CONROGATE_LOG_OUTPUT_FILE_ENABLED=false \
  -p 9000:9000 -p 8080:8080 \
  ghcr.io/chihqiang/conrogate:latest

docker logs -f conrogate     # 查看日志（需 file 输出关闭，见上）
docker rm -f conrogate       # 停止并删除容器（数据仍在 conrogate-data 卷）
```

> SQLite 文件存于命名卷 `conrogate-data`，删除容器不丢数据；改用 MySQL/PostgreSQL 时去掉该卷并设置 `CONROGATE_DB_URL` 即可。

**分离模式 / 迁移（CMD 可覆盖，环境变量用 `-e` 传入）：**

```bash
# 迁移（--seed 写入演示路由；SQLite 同样先迁移再启动）
docker run --rm \
  -e CONROGATE_DB_URL='mysql://conrogate:conrogatepass@host.docker.internal:3306/conrogate' \
  ghcr.io/chihqiang/conrogate:latest conrogate-migrate --seed

# 分离：控制面（:9000）
docker run -d --name conrogate-control \
  -e CONROGATE_DB_URL='mysql://conrogate:conrogatepass@host.docker.internal:3306/conrogate' \
  -e CONROGATE_CONTROL_AUTH_TOKEN=your-secret-token \
  -e CONROGATE_LOG_OUTPUT_FILE_ENABLED=false \
  -p 9000:9000 \
  ghcr.io/chihqiang/conrogate:latest conrogate-control

# 分离：数据面（:8080，HTTP 从 control 拉取配置）
docker run -d --name conrogate-gate \
  -e CONROGATE_GATE_REFRESH_CONFIG_SOURCE=http \
  -e CONROGATE_GATE_REFRESH_CONTROL_API_URL=http://control-host:9000 \
  -e CONROGATE_LOG_OUTPUT_FILE_ENABLED=false \
  -p 8080:8080 \
  ghcr.io/chihqiang/conrogate:latest conrogate-gate
```

> `--env-file` 是**容器内**路径：若使用 `.env.prod`，需挂载宿主文件，如 `-v "$PWD/.env.prod:/app/.env.prod:ro" ghcr.io/chihqiang/conrogate:latest conrogate-gate --env-file /app/.env.prod`；否则直接用上面的 `-e` 传参。

## License

Apache-2.0
