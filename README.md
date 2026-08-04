# Conrogate

Conrogate 轻量级微服务网关，内置配置中心，支持动态路由、负载均衡与流量管控；提供插件扩展机制，采用编译集成模式。

## 特性

- **动态路由**：前缀/精确/正则路径匹配 + Host/Header/Query 多维匹配
- **负载均衡**：轮询 / 加权轮询 / 最少连接 / 一致性哈希
- **流量治理**：限流（固定窗口/滑动窗口/令牌桶）+ 熔断 + 重试 + 超时
- **协议支持**：HTTP/1.1 + HTTP/2 + WebSocket + TCP 隧道
- **插件系统**：静态编译插件 + WASM 扩展点；内置 Log/CORS/Auth 插件
- **控制面**：REST API 管理路由/上游/插件/配置版本，OpenAPI 文档
- **部署灵活**：合并模式（单进程双端口）/ 分离模式（gate × N + control × 1~2）

## 快速开始

```bash
# 1. 启动 PostgreSQL + Redis（基础依赖）
docker compose -f docker-compose.deps.yml up -d

# 2. 迁移 + seed 演示数据
CONROGATE_DB_URL='postgres://conrogate:conrogate_dev@127.0.0.1:5432/conrogate' cargo run -p conrogate-migrate

# 3. 合并模式启动（8080 数据面 + 9000 控制面）
CONROGATE_DB_URL='postgres://conrogate:conrogate_dev@127.0.0.1:5432/conrogate' \
CONROGATE_NODE_AUTO_MIGRATE=false \
CONROGATE_NODE_SEED_DEMO=true \
CONROGATE_CONTROL_AUTH_TOKEN=admin:dev-token:admin \
cargo run -p conrogate

# 4. 验证
curl http://localhost:9000/healthz        # 控制面健康检查
curl http://localhost:8080/demo/hello      # 数据面转发
```

## 三种部署模式

| 模式 | 二进制 | 适用场景 |
|------|--------|----------|
| 合并模式 | `conrogate` | 开发/小规模，单进程双端口 (8080 + 9000) |
| 分离模式 | `conrogate-gate` × N + `conrogate-control` × 1 | 生产/大规模 |
| 迁移工具 | `conrogate-migrate` | 部署前置执行 |

## 工作空间结构

```
conrogate-code/
├── conrogate-contract/        # 契约层：Trait + DTO + 枚举 + Config
├── conrogate-storage/         # 持久化层：SeaORM Entity + 迁移 + 仓储
├── conrogate-balancer/        # 负载均衡：4 种算法 + Registry
├── conrogate-traffic/         # 流量治理：限流 + 熔断 + 重试 + 超时
├── conrogate-plugin/          # 插件框架：注册表 + 管线 + 加载器
├── conrogate-protocol/        # 协议适配层：Handler 抽象 + 注册表 + HTTP/WS/TCP 实现
├── conrogate-gateway/         # 网关核心：路由/代理/遥测/健康检查/配置热载
├── conrogate-control-svc/     # 控制面服务：REST API + 鉴权 + 审计
├── conrogate-plugin-log/      # 官方插件：访问日志
├── conrogate-plugin-cors/     # 官方插件：CORS 跨域
├── conrogate-plugin-auth/     # 官方插件：JWT 鉴权
├── conrogate-migrate/         # 迁移工具 CLI
├── conrogate-gate/            # 数据面二进制
├── conrogate-control/         # 控制面二进制
└── conrogate/                 # 合并模式二进制
```

## 配置

所有配置通过环境变量加载，参考 `.env.example`。关键配置：

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `CONROGATE_DB_URL` | 必填 | 数据库完整连接 URL，前缀决定方言：`postgres://user:pw@host:5432/db`、`mysql://user:pw@host:3306/db`、`sqlite:///path/db.sqlite`（或 `sqlite::memory:`） |
| `CONROGATE_DB_READ_URL` | 主库 URL | 只读库完整 URL（可选）；不设置时复用 `CONROGATE_DB_URL` |
| `CONROGATE_GATE_PORT` | 8080 | 数据面端口 |
| `CONROGATE_CONTROL_LISTEN_PORT` | 9000 | 控制面端口 |
| `CONROGATE_NODE_AUTO_MIGRATE` | false | 自动迁移 |
| `CONROGATE_NODE_SEED_DEMO` | false | 演示数据 |

## API

控制面 REST API 文档：`GET /openapi.json`

| 端点 | 方法 | 说明 |
|------|------|------|
| `/healthz` | GET | 存活探针 |
| `/readyz` | GET | 就绪探针 |
| `/routes` | GET/POST | 路由 CRUD |
| `/upstreams` | GET/POST | 上游 CRUD |
| `/routes/:id/plugins` | GET/POST | 插件绑定 |
| `/config/publish` | POST | 发布配置版本 |
| `/config/rollback` | POST | 回滚配置 |
| `/metrics` | GET | 指标查询 |
| `/events` | GET | 事件查询 |
| `/audit-logs` | GET | 审计日志 |

## 开发

```bash
cargo check --workspace   # 编译检查
cargo test --workspace    # 运行测试
cargo clippy --workspace  # 代码质量
```

## License

Apache-2.0
