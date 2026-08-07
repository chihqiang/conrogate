# 插件文档

> 面向接入人员的官方插件使用说明。插件为**编译集成模式**：随 `conrogate-core` 编译内建，由二进制装配注入网关，绑定配置存在数据库并通过配置热加载下发到数据面。

## 1. 插件总览

当前内置四个官方插件：

| 插件名 | 模块路径 | 协议 | 阻断性 | 作用 |
|--------|----------|------|--------|------|
| `auth` | `conrogate-core/src/plugins/auth/` | HTTP、WebSocket | **阻断** | JWT Bearer Token 鉴权，校验失败返回 401 |
| `cors` | `conrogate-core/src/plugins/cors/` | HTTP | 非阻断 | CORS 跨域响应头注入 + OPTIONS 预检处理 |
| `header_rewrite` | `conrogate-core/src/plugins/header_rewrite/` | HTTP | 非阻断 | 请求 / 响应头改写（set / add / remove，支持占位符） |
| `ip_allow_deny` | `conrogate-core/src/plugins/ip_allow_deny/` | HTTP、WebSocket、TCP | **阻断** | 绑定级 IP allow / deny 访问控制，拒绝返回 403 |

- **阻断性**：阻断插件（`blocking = true`）可在请求阶段直接终止请求（如鉴权失败返回 401）；非阻断插件只记录 / 改响应头，永不拦截。
- **每绑定独立实例**：插件配置按「路由绑定」隔离，同一插件绑定到不同路由可配置不同的密钥 / 白名单 / 跳过规则，互不干扰。
- **执行顺序**：一个路由可绑定多个插件，按绑定的 `order` **升序**执行；`before_request` 中任一阻断插件返回终止响应，后续插件不再执行；`after_response` 对所有命中协议且已执行的插件按序执行。
- **协议匹配**：插件只对声明支持的协议生效（如 `cors` 仅 HTTP，WS/TCP 路由上绑定 cors 会被跳过）。

### 1.1 绑定 / 解绑 / 更新

插件绑定挂在路由下，通过控制面 API 管理：

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/v1/routes/:route_id/plugins` | GET | 查看路由的插件绑定列表 |
| `/api/v1/routes/:route_id/plugins` | POST | 绑定插件 |
| `/api/v1/routes/:route_id/plugins/:plugin_name` | PUT | 更新绑定（config / order / enabled） |
| `/api/v1/routes/:route_id/plugins/:plugin_name` | DELETE | 解绑插件 |

**绑定请求体：**

| 字段 | 类型 | 必填 | 默认 | 说明 |
|------|------|------|------|------|
| `plugin_name` | string | 是 | — | 插件名：`cors` / `auth` / `header_rewrite` / `ip_allow_deny` |
| `config` | object/null | 否 | `null` | 插件配置 JSON；`null` 表示使用默认配置 |
| `order` | int | 否 | `0` | 执行顺序，升序执行（数值小先执行） |
| `blocking` | bool | 否 | `false` | 是否为阻断插件（鉴权类建议 `true`） |
| `enabled` | bool | 否 | `true` | `false` 时该绑定不参与执行 |

> 配置合法性（JSON 结构、必填字段）在绑定/更新时**即时校验**，非法配置会直接拒绝该操作，避免阻断插件（如 auth）被静默跳过。

### 1.2 如何生效

- `CONROGATE_GATE_REFRESH_CONFIG_SOURCE=db`（默认）/ `http`：改表即生效，数据面按轮询间隔（默认 5s）热载，原子替换插件链，**无需重启**。
- `redis`：**必须执行发布** `POST /api/v1/configs/publish` 才会推送新快照到数据面。
- 无论哪种模式都建议在变更后发布：生成不可变版本号，便于留档、diff 与回滚（见 `docs/operations.md`）。

---

## 2. `auth` — JWT Bearer Token 鉴权插件

- **插件名**：`auth`
- **协议**：HTTP、WebSocket（WS 升级握手阶段完成校验，未通过不建立隧道）
- **阻断性**：`blocking = true`（校验失败直接终止请求，返回 `401`）
- **是否需要请求体**：否（只读取 `Authorization` 头，路由保持流式透传路径）

### 2.1 原理

按路由绑定级配置构建独立的 JWT 验证器：

| 算法族 | 密钥来源 | 适用场景 |
|--------|----------|----------|
| HS256 / HS384 / HS512 | `secret`（HMAC 对称密钥） | 自签 token、网关与上游共享密钥 |
| RS256 / RS384 / RS512 | `rsa_pem`（静态 RSA 公钥 PEM） | 上游独立签发的 token |
| RS256（JWKS） | `jwks_url`（远程密钥集） | 兼容 OIDC / Keycloak / Auth0 |

验证要点：

- **签名校验**：按 token 头声明的 `alg` 验签，不支持 `alg=none`，杜绝降级攻击。
- **过期检查**：`exp` 过期直接拒绝（`validate_exp` 固定开启）。
- **可选签发者 / 受众校验**：配置 `issuer` / `audience` 后逐项校验。
- **JWKS 缓存**：按 `kid` 缓存远程密钥，TTL 默认 300s；拉取失败时若缓存未过期则继续使用旧密钥（stale-while-error），不阻塞多实例。

校验失败统一返回 HTTP `401`，响应体 `{"code":10002,"msg":"unauthorized: ..."}`，并携带网关 `x-trace-id` 便于排查。

### 2.2 配置字段

| 字段 | 类型 | 必填 | 默认 | 说明 |
|------|------|------|------|------|
| `algorithm` | string | 否 | `HS256` | `HS256` / `HS384` / `HS512` / `RS256` / `RS384` / `RS512` |
| `secret` | string | HMAC 时必填 | `""` | HMAC 对称密钥 |
| `rsa_pem` | string | RS256 静态密钥时二选一 | 无 | RSA 公钥 PEM（须含 `-----BEGIN PUBLIC KEY-----`） |
| `jwks_url` | string | RS256 动态密钥时二选一 | 无 | JWKS 远程密钥集 URL |
| `jwks_cache_ttl_seconds` | int | 否 | `300` | JWKS 缓存 TTL（秒） |
| `issuer` | string | 否 | 无 | 设置后强制校验 `iss` |
| `audience` | string | 否 | 无 | 设置后强制校验 `aud` |
| `require_token` | bool | 否 | `true` | `false` 时无 token 也放行（有 token 仍会校验） |

配置校验在绑定 API 即时执行：HMAC 算法缺 `secret`、RSA 算法缺 `rsa_pem`/`jwks_url`（且 `require_token=true`）都会拒绝绑定。

### 2.3 绑定示例

HS256：

```bash
curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
  -H 'Authorization: Bearer <token>' -H 'Content-Type: application/json' \
  -d '{
    "plugin_name": "auth",
    "config": {
      "algorithm": "HS256",
      "secret": "my-secret",
      "require_token": true
    },
    "order": 0,
    "blocking": true,
    "enabled": true
  }'
```

RS256（静态公钥）：

```bash
curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
  -H 'Authorization: Bearer <token>' -H 'Content-Type: application/json' \
  -d '{
    "plugin_name": "auth",
    "config": {
      "algorithm": "RS256",
      "rsa_pem": "-----BEGIN PUBLIC KEY-----\n...\n-----END PUBLIC KEY-----"
    },
    "order": 0,
    "blocking": true,
    "enabled": true
  }'
```

RS256（JWKS）：

```bash
curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
  -H 'Authorization: Bearer <token>' -H 'Content-Type: application/json' \
  -d '{
    "plugin_name": "auth",
    "config": {
      "algorithm": "RS256",
      "jwks_url": "https://auth.example.com/.well-known/jwks.json",
      "jwks_cache_ttl_seconds": 300
    },
    "order": 0,
    "blocking": true,
    "enabled": true
  }'
```

> JWKS 模式要求 token 携带 `kid` 头；JWKS 响应中按 `kid` 匹配 RSA 公钥。

### 2.4 验证

```bash
# 无 token → 401
curl -i http://<网关>:8080/your/path

# 生成 HS256 token（Python 示例，secret 与绑定配置一致）
python3 - <<'EOF'
import hmac, hashlib, base64, json
def b64(b): return base64.urlsafe_b64encode(b).rstrip(b"=").decode()
h = b64(json.dumps({"alg":"HS256","typ":"JWT"}).encode())
p = b64(json.dumps({"sub":"123","exp":4102444800}).encode())
s = hmac.new(b"my-secret", f"{h}.{p}".encode(), hashlib.sha256).digest()
print(f"{h}.{p}.{b64(s)}")
EOF

# 携带有效 token → 200
curl -i -H "Authorization: Bearer <token>" http://<网关>:8080/your/path
```

### 2.5 注意事项

- 不同路由绑定不同 `secret`/`algorithm` 是**隔离**的（每绑定独立实例），可放心分别配置。
- 插件不读取请求体，不产生缓冲影响，对 SSE 等长连接流友好。
- WS 路由绑定后，升级握手阶段即完成鉴权，未通过不会建立隧道。

---

## 3. `cors` — CORS 跨域插件

- **插件名**：`cors`
- **协议**：HTTP
- **阻断性**：`blocking = false`（正常请求不拦截；仅 OPTIONS 预检在网关节内处理）
- **是否需要请求体**：否

### 3.1 原理

在网关层统一处理浏览器跨域，避免上游各自配置：

- **OPTIONS 预检请求**：在 `before_request` 阶段直接拦截，返回 `204 No Content` 并注入 CORS 响应头，**不转发给上游**。
- **正常请求**：在 `after_response` 阶段向真实响应注入 `Access-Control-Allow-Origin` 等头，透传上游结果。

Origin 匹配策略：

- 配置含 `*` → 返回 `Access-Control-Allow-Origin: *`。
- 否则按白名单**精确匹配**请求 `Origin`，命中后**回显该 Origin**（CORS 规范禁止返回多值列表，不会拼接多个域名）。
- 未命中 → 不注入 CORS 头（浏览器会拦截跨域响应）。

### 3.2 配置字段

| 字段 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `allow_origins` | string[] | `["*"]` | 允许的 Origin 白名单 |
| `allow_methods` | string[] | `["GET","POST","PUT","PATCH","DELETE","OPTIONS"]` | 预检响应 `Access-Control-Allow-Methods` |
| `allow_headers` | string[] | `["Content-Type","Authorization"]` | 预检响应 `Access-Control-Allow-Headers` |
| `expose_headers` | string[] | `[]` | `Access-Control-Expose-Headers`（允许前端读取的响应头） |
| `allow_credentials` | bool | `false` | 是否允许携带 Cookie（`Access-Control-Allow-Credentials: true`） |
| `max_age_seconds` | int | `3600` | 预检缓存时长 `Access-Control-Max-Age` |

> 注意：`allow_credentials=true` 时浏览器要求 `Access-Control-Allow-Origin` 不能是 `*`，应配置为具体域名。

### 3.3 绑定示例

```bash
curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
  -H 'Authorization: Bearer <token>' -H 'Content-Type: application/json' \
  -d '{
    "plugin_name": "cors",
    "config": {
      "allow_origins": ["https://app.example.com", "https://admin.example.com"],
      "allow_methods": ["GET","POST","OPTIONS"],
      "allow_headers": ["Content-Type","Authorization","X-Custom"],
      "expose_headers": ["X-Trace-Id","X-Request-Id"],
      "allow_credentials": true,
      "max_age_seconds": 600
    },
    "order": 0,
    "blocking": false,
    "enabled": true
  }'
```

### 3.4 验证

```bash
# 预检请求 → 204 + CORS 头（不触达上游）
curl -i -X OPTIONS http://<网关>:8080/your/path \
  -H "Origin: https://app.example.com" \
  -H "Access-Control-Request-Method: POST"

# 正常请求 → 200 + Access-Control-Allow-Origin 回显
curl -i http://<网关>:8080/your/path -H "Origin: https://app.example.com"
```

### 3.5 注意事项

- 未命中白名单的 Origin：预检返回 204 但**不带** CORS 头（浏览器拒绝跨域读取），正常请求响应也不注入 CORS 头。
- 插件不读取请求体，不影响流式转发路径。
- 仅支持 HTTP 协议路由；WS / TCP 路由上绑定 cors 会被跳过。

---

## 4. `header_rewrite` — 请求 / 响应头改写插件

- **插件名**：`header_rewrite`
- **协议**：HTTP
- **阻断性**：`blocking = false`（只改写头，永不拦截请求）
- **是否需要请求体**：否

### 4.1 原理

按路由绑定级配置，在请求转发前（`before_request`）改写请求头、在响应回包前（`after_response`）改写响应头，全程不拦截、不读取请求体。

配置分 `request` / `response` 两段，每段支持三类操作：

| 操作 | 语义 |
|------|------|
| `set` | 覆盖同名头的所有值；头不存在则新增 |
| `add` | 追加一个值，不覆盖已有值（同名字头可共存） |
| `remove` | 删除该头（存在即移除） |

`set` / `add` 的值支持占位符，运行时替换为真实上下文：

| 占位符 | 含义 |
|--------|------|
| `$client_ip` | 客户端 IP |
| `$request_id` | 请求 ID |
| `$trace_id` | 链路 trace ID |
| `$route_id` | 命中的路由 ID |
| `$method` | 请求方法（仅请求段有效，响应段为空串） |
| `$path` | 请求路径（仅请求段有效，响应段为空串） |

### 4.2 配置字段

| 字段 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `request.set` | object | `{}` | 覆盖请求头，键为头名，值为新值 |
| `request.add` | object | `{}` | 追加请求头值 |
| `request.remove` | string[] | `[]` | 删除的请求头名列表 |
| `response.set` | object | `{}` | 覆盖响应头 |
| `response.add` | object | `{}` | 追加响应头值 |
| `response.remove` | string[] | `[]` | 删除的响应头名列表 |

> 头名必须为合法的 HTTP 头名；值不能包含 CR / LF 等控制字符（防响应头注入）。

### 4.3 绑定示例

```bash
curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
  -H 'Content-Type: application/json' \
  -d '{
    "plugin_name": "header_rewrite",
    "config": {
      "request": {
        "set": { "X-Real-IP": "$client_ip", "X-Gateway": "conrogate" },
        "add": { "X-Custom": "value" },
        "remove": ["X-Internal-Token"]
      },
      "response": {
        "set": { "X-Powered-By": "conrogate" },
        "remove": ["X-Debug"]
      }
    },
    "order": 0,
    "blocking": false,
    "enabled": true
  }'
```

### 4.4 验证

```bash
curl -i http://<网关>:8080/your/path
# 响应头出现 X-Powered-By: conrogate，且不再有 X-Debug
```

### 4.5 注意事项

- 插件不读取请求体，不影响流式转发路径。
- `remove` 优先于 `set` / `add` 执行；未识别的占位符按原样透传。

---

## 5. `ip_allow_deny` — IP 访问控制插件

- **插件名**：`ip_allow_deny`
- **协议**：HTTP、WebSocket（升级握手阶段）、TCP 隧道（连接建立阶段）
- **阻断性**：`blocking = true`（拒绝时直接终止，返回 `403`）
- **是否需要请求体**：否

### 5.1 原理

按路由绑定级配置，对客户端 IP（`ctx.client_ip`，已按可信代理链路解析出的**真实 IP**）做 allow / deny 访问控制：

| 配置 | 语义 |
|------|------|
| `deny` 非空且命中 | 一律拒绝（**deny 优先**，即使同时命中 allow） |
| `allow` 非空且未命中 | 拒绝 |
| `allow` / `deny` 均为空 | 配置非法，绑定直接被拒 |
| `allow` 为空（无白名单） | 仅按 deny 拦截 |

拒绝时返回 HTTP `403`，响应体 `{"code":10003,"msg":"forbidden: ip not allowed"}`（与全局 IP 黑名单保持一致）。TCP 隧道在连接建立阶段（`on_connect`）即被拒绝。

### 5.2 配置字段

| 字段 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `allow` | string[] | `[]` | 仅允许的 IP / 网段列表；为空表示不启用白名单 |
| `deny` | string[] | `[]` | 拒绝的 IP / 网段列表；为空表示不启用黑名单 |

> 元素为 IP 或 CIDR 网段，支持 IPv4 / IPv6，裸 IP 视作 /32 或 /128。配置校验在绑定 API 即时执行：任一条目解析失败、或 allow/deny 同时为空都会拒绝绑定。

### 5.3 绑定示例

```bash
curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
  -H 'Content-Type: application/json' \
  -d '{
    "plugin_name": "ip_allow_deny",
    "config": {
      "allow": ["10.0.0.0/8", "192.168.1.0/24"],
      "deny": ["10.20.0.0/16"]
    },
    "order": 0,
    "blocking": true,
    "enabled": true
  }'
```

### 5.4 验证

```bash
# 来自 10.20.0.5（命中 deny）→ 403
curl -i -H "X-Forwarded-For: 10.20.0.5" http://<网关>:8080/your/path

# 来自 10.1.0.5（allow 内、deny 外）→ 放行
curl -i -H "X-Forwarded-For: 10.1.0.5" http://<网关>:8080/your/path
```

### 5.5 注意事项

- 该插件与全局 IP 黑名单**互相独立**：全局黑名单在任何路由前先拦截，命中即 403；本插件提供**绑定级**（按路由）更细粒度的 allow/deny 控制。
- 插件不读取请求体，路由保持流式转发路径。

---

## 6. 组合与最佳实践

- **先鉴权后跨域**：同一路由同时绑定 `auth` 与 `cors` 时，建议 `auth.order` 小于 `cors.order`（auth 先执行），避免未鉴权请求先拿到 CORS 头。
- **IP 管控优先**：`ip_allow_deny` 应绑定在插件链最前面（`order` 最小），先做 IP 准入再进入鉴权 / 头改写，避免对非授权 IP 浪费下游插件开销。
- **CORS + 凭据**：前端需要携带 Cookie（`Authorization` 或 `credentials`）时，`allow_credentials=true` 且 `allow_origins` 必须为具体域名，不能是 `*`。
- **WebSocket**：`auth` 在 WS 升级握手阶段完成鉴权，未通过不建立隧道；`cors` 不适用于 WS。
- **配置留档**：绑定/更新插件后建议执行 `POST /api/v1/configs/publish` 发布配置版本，便于审计与回滚（`redis` 模式必须发布才生效）。
- **排障**：绑定返回 `20003 插件配置非法` 时按 `msg` 修正 config；数据面未生效时确认发布 / 轮询间隔。

## 7. 相关文档

- 配置绑定 API 细节 → `docs/api.md`
- 配置版本发布 / 回滚 → `docs/operations.md`
- 全局 IP 黑名单（基础设施层）→ `docs/security.md`
- 插件体系代码入口 → `conrogate-core/src/contract/plugin.rs`（`Plugin` trait）与 `conrogate-core/src/plugin/loader.rs`（链构建）
