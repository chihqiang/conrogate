# conrogate-plugin-cors

CORS 跨域插件（Conrogate 官方内置插件）。

- 插件名：`cors`
- 协议：HTTP
- 阻断性：`blocking = false`
- 是否需要请求体：否

## 原理

在网关层统一处理浏览器跨域，避免上游各自配置。核心逻辑：

- **OPTIONS 预检请求**：在 `before_request` 阶段直接拦截，返回 `204 No Content` 并注入 CORS 响应头，**不再转发给上游**。
- **正常请求**：在 `after_response` 阶段向真实响应注入 `Access-Control-Allow-Origin` 等头，透传上游结果。

Origin 匹配策略（`resolve_origin`）：

- 配置含 `*` → 直接返回 `Access-Control-Allow-Origin: *`。
- 否则按白名单**精确匹配**请求 `Origin`，命中后**回显该 Origin**（CORS 规范禁止返回多值列表，所以不会拼接多个域名）。
- 未命中 → 不注入 CORS 头（浏览器会拦截响应）。

每个绑定拥有独立配置实例，不同路由可配置不同的跨域策略。

## 请求过程

```text
客户端（浏览器）→ 网关
 ┌─ 预检请求 OPTIONS
 │    cors.before_request
 │      ├─ 命中白名单 Origin → 返回 204 + CORS 头（不转发上游）
 │      └─ 未命中            → 直接终止（无 CORS 头）
 │
 └─ 正常请求（GET/POST/...）
      cors.before_request → 放行（非 OPTIONS）
      转发到上游
      cors.after_response → 注入 Access-Control-Allow-Origin 等头
      返回客户端
```

## 配置

绑定到路由时，`config` 支持以下字段：

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `allow_origins` | string[] | `["*"]` | 允许的 Origin 白名单 |
| `allow_methods` | string[] | GET/POST/PUT/PATCH/DELETE/OPTIONS | 预检响应 `Access-Control-Allow-Methods` |
| `allow_headers` | string[] | `["Content-Type","Authorization"]` | 预检响应 `Access-Control-Allow-Headers` |
| `expose_headers` | string[] | `[]` | 响应 `Access-Control-Expose-Headers`（允许前端读取的响应头） |
| `allow_credentials` | bool | `false` | 是否允许携带 Cookie（`Access-Control-Allow-Credentials: true`） |
| `max_age_seconds` | int | `3600` | 预检缓存时长 `Access-Control-Max-Age` |

> 注意：`allow_credentials=true` 时浏览器要求 `Access-Control-Allow-Origin` 不能是 `*`，应配置为具体域名。

## 使用

### 1. 绑定插件到路由

```bash
curl -X POST http://<控制面>:9000/api/v1/routes/:route_id/plugins \
  -H 'Content-Type: application/json' \
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

### 2. 发布配置

```bash
curl -X POST http://<控制面>:9000/api/v1/configs/publish
```

### 3. 验证

```bash
# 预检请求 → 204 + CORS 头（不触达上游）
curl -i -X OPTIONS http://<网关>:8080/your/path \
  -H "Origin: https://app.example.com" \
  -H "Access-Control-Request-Method: POST"

# 正常请求 → 200 + Access-Control-Allow-Origin 回显
curl -i http://<网关>:8080/your/path -H "Origin: https://app.example.com"
```

## 注意事项

- 未命中白名单的 Origin：预检返回 204 但**不带** CORS 头（浏览器拒绝跨域读取），正常请求响应也不注入 CORS 头。
- 插件不读取请求体，不影响流式转发路径。
