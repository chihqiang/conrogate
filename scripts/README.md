# scripts 目录说明

Conrogate 的开发 / 测试脚本。包含本地质量门禁、网关启动辅助、以及覆盖核心能力的集成测试套件。

```text
scripts/
├── check.sh                 # 本地 CI 质量门禁（fmt / clippy / build / test / doc）
├── dev-up.sh                # 启动依赖(PostgreSQL/Redis) + 迁移 + 合并模式网关
├── lib-conrogate.sh         # 集成测试公共函数库（被 tests/*.sh source）
├── tests/                   # 集成测试套件（每个脚本可独立运行）
│   ├── test-concurrency.sh
│   ├── test-config-rollback.sh
│   ├── test-header-condition.sh
│   ├── test-httpbin-svc.sh
│   ├── test-ip-blacklist.sh
│   ├── test-plugin-cors.sh
│   ├── test-plugin-header-rewrite.sh
│   ├── test-plugin-ip-allow-deny.sh
│   ├── test-sse-svc.sh
│   └── test-ws-svc.sh
└── upstream/                # 本地测试上游（PHP + Swoole）
    ├── echo.php             # HTTP 回显服务：回显 method/path/query/headers/body
    ├── sse.php              # SSE 流式服务：定时逐条输出 text/event-stream
    └── ws.php               # WebSocket echo 服务（含测试客户端）
```

## 前置条件

- 合并模式网关已启动（数据面 `8080` + 控制面 `9000`）：

  ```bash
  ./scripts/dev-up.sh
  ```

- 测试脚本依赖 `curl`、`jq`（缺省回退 `python3`）、`php`（含 Swoole 扩展，仅 SSE/WS 套件需要）。

## 公共环境变量

以下变量对所有 `tests/*.sh` 生效（脚本内已提供默认值）：

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `CONROGATE_CONTROL_BASE` | `http://127.0.0.1:9000/api/v1` | 控制面地址 |
| `CONROGATE_GATE_BASE` | `http://127.0.0.1:8080` | 数据面（网关）地址 |
| `CONROGATE_CONTROL_AUTH_TOKEN` | 空（无鉴权） | 控制面鉴权 token，与启动配置一致 |

启动 `dev-up.sh` 时已设置 `CONROGATE_CONTROL_AUTH_TOKEN=admin:dev-token:admin`。

## 运行方式

### 质量门禁

```bash
./scripts/check.sh
```

执行 `cargo fmt --check`、`cargo clippy -D warnings`、`cargo deny`、全量 build / test / doc、`cargo machete`。

### 运行全部测试套件

```bash
for t in scripts/tests/test-*.sh; do "$t"; done
```

### 运行单个套件

```bash
./scripts/tests/test-plugin-cors.sh
```

所有套件运行完成后会创建示例上游 / 路由并发布；如需清理这些示例配置：

```bash
./scripts/tests/test-plugin-cors.sh --cleanup
```

## 套件清单

| 脚本 | 验证内容 |
| --- | --- |
| `test-plugin-cors.sh` | 绑定 cors 插件：OPTIONS 预检拦截（204 + CORS 头）、白名单外 Origin 不带 CORS 头、正常请求响应头注入 |
| `test-plugin-header-rewrite.sh` | header_rewrite 插件：请求头 set / add / remove + 占位符，响应头 set / remove |
| `test-plugin-ip-allow-deny.sh` | ip_allow_deny 插件：deny 命中 403、allow 白名单放行 / 未命中 403 |
| `test-header-condition.sh` | 路由多维条件匹配：header / query 精确匹配 + 优先级回落（3 上游分流） |
| `test-ip-blacklist.sh` | 全局 IP 黑名单：拉黑 127.0.0.1 即时拦截 → 解封恢复（脚本结束保证解封） |
| `test-sse-svc.sh` | SSE 流式透传：正文与直连一致、首字节远早于整条流结束（证明流式而非缓冲） |
| `test-ws-svc.sh` | WebSocket 隧道：经网关升级后回显消息校验 |
| `test-httpbin-svc.sh` | 接入示例：把 `https://httpbin.org` 注册为上游，按 `/anything` 前缀转发验证 |
| `test-concurrency.sh` | 并发转发：100 个请求（并行度 20）经网关全部成功且响应一致 |
| `test-config-rollback.sh` | 配置版本回滚：发布 → 生效 → 回滚到发布前版本 → 路由从网关移除 |

### 上游工具

| 脚本 | 说明 | 直接运行 |
| --- | --- | --- |
| `upstream/echo.php` | HTTP 回显服务器，`--add-resp-header k:v` 可附加响应头供响应头校验 | `php upstream/echo.php [--port 0]` |
| `upstream/sse.php` | SSE 服务器，可调 `--count` 事件条数与 `--delay-ms` 间隔 | `php upstream/sse.php [--port 0]` |
| `upstream/ws.php` | WS echo 服务器 + 客户端 | 服务端 `php upstream/ws.php`；客户端 `php upstream/ws.php --client ws://host:port/path` |

这些 PHP 文件由测试套件按需自动启动到随机端口（端口号打印到 stdout）并在退出时清理，一般无需手动运行。
