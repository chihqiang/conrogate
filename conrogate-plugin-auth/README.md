# conrogate-plugin-auth

JWT Bearer Token 鉴权插件（Conrogate 官方内置插件）。

- 插件名：`auth`
- 协议：HTTP、WebSocket（WS 升级握手阶段校验）
- 阻断性：`blocking = true`（校验失败直接终止请求，返回 401）
- 是否需要请求体：否（仅读取 `Authorization` 头，转发走流式透传路径）

## 原理

按路由绑定级配置构建 JWT 验证器，每个绑定拥有**独立配置实例**（配置隔离，互不干扰）。支持：

| 算法族 | 密钥来源 | 适用场景 |
| --- | --- | --- |
| HS256 / HS384 / HS512 | `secret`（HMAC 对称密钥） | 自签 token、网关与上游共享密钥 |
| RS256 / RS384 / RS512 | `rsa_pem`（静态 RSA 公钥 PEM） | 上游独立签发的 token |
| RS256（JWKS） | `jwks_url`（远程密钥集） | 兼容 OIDC / Keycloak / Auth0 |

验证要点：

- **签名校验**：使用 `jsonwebtoken` 按 token 头声明的 `alg` 验签，杜绝 `alg=none` 等降级攻击（不支持 `none`）。
- **过期检查**：`exp` 过期直接拒绝（`validate_exp = true`）。
- **可选签发者/受众校验**：配置 `issuer` / `audience` 后逐项校验。
- **JWKS 缓存**：远程 JWKS 拉取后按 `kid` 缓存，TTL 默认 300 秒；拉取失败时若缓存未过期则继续使用旧密钥（stale-while-error），且不阻塞多实例。

## 请求过程

```text
客户端 → 网关
 1. 路由匹配（HTTP / WS 升级前阶段）
 2. preflight：从管线缓存取该路由插件链（按绑定 order 升序执行）
 3. auth.before_request
     ├─ 无 Authorization: Bearer <token>
     │    ├─ require_token=true  → 401 {"code":10002,"msg":"unauthorized: missing bearer token"}
     │    └─ require_token=false  → 放行
     ├─ 有 token：
     │    ├─ 解码 JWT 头失败        → 401 10002 invalid token header
     │    ├─ 密钥解析失败（无 kid/密钥缺失）→ 401 10002
     │    ├─ 验签/过期/iss/aud 失败  → 401 10002 unauthorized: <原因>
     │    └─ 校验通过               → 放行
 4. 放行后继续：限流 → 选节点 → 转发
```

校验失败时返回 HTTP 401，响应体为 `{"code":10002,"msg":"unauthorized: ..."}`，并携带网关 `x-trace-id` 便于排查。

## 配置

绑定到路由时，`config` 支持以下字段：

| 字段 | 类型 | 必填 | 默认 | 说明 |
| --- | --- | --- | --- | --- |
| `algorithm` | string | 否 | `HS256` | `HS256/HS384/HS512/RS256/RS384/RS512` |
| `secret` | string | HMAC 时必填 | 空 | HMAC 对称密钥 |
| `rsa_pem` | string | RS256 静态时二选一 | 无 | RSA 公钥 PEM（需含 `-----BEGIN PUBLIC KEY-----`） |
| `jwks_url` | string | RS256 动态时二选一 | 无 | JWKS 远程 URL |
| `jwks_cache_ttl_seconds` | int | 否 | `300` | JWKS 缓存 TTL（秒） |
| `issuer` | string | 否 | 无 | 强制校验 `iss` |
| `audience` | string | 否 | 无 | 强制校验 `aud` |
| `require_token` | bool | 否 | `true` | `false` 时无 token 也放行（token 存在则仍校验） |

配置校验在绑定 API 即时执行：HMAC 算法缺 `secret`、RSA 缺 `rsa_pem/jwks_url`（且 `require_token=true`）都会直接拒绝绑定。

## 使用

### 1. 绑定插件到路由

```bash
curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
  -H 'Content-Type: application/json' \
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

RS256（静态公钥）示例：

```bash
curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
  -H 'Content-Type: application/json' \
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

### 2. 发布配置

```bash
curl -X POST http://<控制面>:9000/api/v1/configs/publish
```

网关数据面通过 DB 轮询热载（约 5s），原子替换插件链，无需重启。

### 3. 验证

```bash
# 无 token → 401
curl -i http://<网关>:8080/your/path

# 生成 HS256 token（Python 示例）
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

## 注意事项

- 不同路由绑定不同 `secret`/`algorithm` 是**隔离**的（每绑定独立实例），可放心分别配置。
- 该插件不读取请求体，路由保持流式转发路径；对 SSE 等长连接流不产生缓冲影响。
- WS 路由绑定后，升级握手阶段即完成鉴权，未通过不会建立隧道。
