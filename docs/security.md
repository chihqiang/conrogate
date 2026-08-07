# 安全文档：全局 IP 黑名单

> 面向运维 / 安全人员的全局 IP 黑名单使用说明。黑名单是**基础设施层**能力（非插件），由控制面 API 管理、数据面热载生效，对 HTTP / WebSocket / TCP 隧道三协议统一拦截。

## 1. 设计

| 项 | 说明 |
|----|------|
| 生效位置 | 数据面在**路由匹配前**拦截（HTTP/WS 请求、TCP 连接建立阶段），任何路由命中即拒绝 |
| 拦截语义 | 命中即返回 `403`，响应体 `{"code":10003,"msg":"forbidden: ip not allowed"}` |
| 匹配粒度 | IP 或 CIDR 网段（IPv4 / IPv6；裸 IP 视作 /32 或 /128） |
| 过期机制 | 每条目带 `expires_at`（`null` = 永久），到期自动失效（数据面内存态过滤 + DB 清理） |
| 热载 | 数据面按配置轮询间隔同步（DB 直连 / HTTP 拉取两种模式均支持），**无需重启** |
| 失败策略 | 拉取失败保持当前黑名单（fail-open，不因拉取错误清空导致流量中断） |
| 审计 | 拉黑 / 解拉黑操作写入审计日志（`action=create/delete`，`resource=ip_blacklist`） |

> 黑名单为全局生效；如需**按路由**做更细粒度的 IP 准入，使用 `ip_allow_deny` 插件（见 `docs/plugins.md`）。

## 2. API

管理端点挂载在控制面 `api_prefix`（默认 `/api/v1`），需要 `Authorization: Bearer <token>`：

| 端点 | 方法 | 权限 | 说明 |
|------|------|------|------|
| `/api/v1/security/ip_blacklist?page=&page_size=&keyword=` | GET | Viewer+ | 黑名单列表（分页 + 按 IP/CIDR/备注模糊搜索） |
| `/api/v1/security/ip_blacklist` | POST | Operator+ | 拉黑 |
| `/api/v1/security/ip_blacklist/:id` | DELETE | Operator+ | 解除拉黑 |

**拉黑请求体：**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `ip_or_cidr` | string | 是 | IP 或 CIDR 网段，如 `1.2.3.4`、`10.0.0.0/24`、`2001:db8::/32` |
| `reason` | string | 否 | 拉黑原因 / 备注 |
| `expires_in_seconds` | int | 否 | 拉黑时长（秒）；缺省 = 永久。传 `0` 会被拒绝 |

> 幂等：重复拉黑同一 IP/CIDR 不会报错，而是刷新 `reason` / `expires_at`（变成「续期」）。

## 3. 使用示例

```bash
TOKEN='<控制面 Token>'
BASE='http://<控制面>:9000/api/v1'

# 永久拉黑单个 IP
curl -X POST "$BASE/security/ip_blacklist" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"ip_or_cidr": "1.2.3.4", "reason": "恶意扫描"}'

# 拉黑一个网段 10 小时
curl -X POST "$BASE/security/ip_blacklist" \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"ip_or_cidr": "10.0.0.0/24", "reason": "撞库", "expires_in_seconds": 36000}'

# 列表（按关键字搜索）
curl "$BASE/security/ip_blacklist?keyword=10.0.0&page=1&page_size=20" \
  -H "Authorization: Bearer $TOKEN"

# 解除拉黑（:id 为列表返回的条目 id）
curl -X DELETE "$BASE/security/ip_blacklist/3" -H "Authorization: Bearer $TOKEN"
```

**数据面验证：**

```bash
# 来自 1.2.3.4 → 403
curl -i -H "X-Forwarded-For: 1.2.3.4" http://<网关>:8080/any/path
```

## 4. 注意事项

- 拉黑是**全局**的，会同时影响所有路由与三个协议；想放行个别路由请用 `ip_allow_deny` 插件。
- 客户端 IP 按可信代理链路解析（`trusted_proxies`），伪造 `X-Forwarded-For` 不会绕过黑名单。
- 数据面默认每轮询周期（默认 5s）同步一次黑名单，拉黑后数秒内生效。
- 黑名单管理 API 的写操作（拉黑/解拉黑）与列表查询均在审计日志可查。
