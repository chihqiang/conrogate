#!/usr/bin/env bash
set -euo pipefail

# header_rewrite 插件测试：校验请求头改写（set/add/remove + 占位符）与响应头改写（set/remove）。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-plugin-header-rewrite.sh            # 起 echo 上游 → 绑定 header_rewrite → 发布 → 校验
#   ./scripts/tests/test-plugin-header-rewrite.sh --cleanup  # 删除示例配置
#
# 可覆盖的环境变量：同 lib-conrogate.sh

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"

UP="hrw-up"
ROUTE="hrw-route"

stop_all() { stop_echo_server; }

cleanup() {
  echo "== 清理示例配置 =="
  local deleted=0 id
  id=$(route_id_by_name "$ROUTE")
  if [ -n "$id" ]; then
    api DELETE "/routes/${id}" >/dev/null
    echo "  [OK] 路由 $ROUTE (id=$id) 已删除"
    deleted=1
  fi
  id=$(upstream_id_by_name "$UP")
  if [ -n "$id" ]; then
    api DELETE "/upstreams/${id}" >/dev/null
    echo "  [OK] 上游 $UP (id=$id) 已删除"
    deleted=1
  fi
  [ "$deleted" = "1" ] || echo "  [SKIP] 未找到示例配置"
  echo "  [提示] 删除需重新发布生效"
  curl -sS -X POST "${BASE}/configs/publish" ${AUTH[@]+"${AUTH[@]}"} >/dev/null 2>&1 || true
  exit 0
}

[ "${1:-}" = "--cleanup" ] && cleanup
trap stop_all EXIT

echo "== 1. 前置体检 =="
health_check

echo "== 2. 启动本地 echo 上游（附加响应头 X-Debug 供 remove 校验） =="
start_echo_server --add-resp-header "X-Debug: secret"
PORT=$ECHO_PORT
echo "  [OK] 上游 127.0.0.1:$PORT（响应头含 X-Debug: secret）"

echo "== 3. 注册上游 + 路由 =="
UP_ID=$(ensure_upstream "$UP" "127.0.0.1:$PORT")
ROUTE_ID=$(ensure_route "$ROUTE" '{"path":{"prefix":"/rw"}}' "$UP_ID" 10)
echo "  [OK] 上游 id=$UP_ID 路由 id=$ROUTE_ID"

echo "== 4. 绑定 header_rewrite 插件 =="
bind_plugin "$ROUTE_ID" "header_rewrite" "{
  \"request\": {
    \"set\": { \"X-Gateway\": \"conrogate\", \"X-Real-IP\": \"\$client_ip\" },
    \"add\": { \"X-Custom\": \"added\" },
    \"remove\": [\"X-Internal\"]
  },
  \"response\": {
    \"set\": { \"X-Powered-By\": \"conrogate\" },
    \"remove\": [\"X-Debug\"]
  }
}"
echo "  [OK] header_rewrite 已绑定"

echo "== 5. 发布配置 =="
publish_config "header-rewrite-test"
wait_http 200 "/rw" >/dev/null
echo "  [OK] 网关路由已生效"

echo "== 6. 请求头改写校验 =="
# 客户端发送：X-Internal(应被剥离)、X-Real-IP 伪造值(应被覆盖为真实 client_ip)
body=$(curl -sS -m 5 "${GATE}/rw" -H "X-Internal: should-remove" -H "X-Real-IP: 1.2.3.4")

req_hdr() { jget ".headers.\"$1\"" <<<"$body"; }

[ "$(req_hdr 'x-gateway')" = "conrogate" ] && echo "  [OK] set：上游收到 X-Gateway=conrogate" \
  || { echo "  [FAIL] x-gateway 应为 conrogate，实际 $(req_hdr x-gateway)" >&2; exit 1; }

[ "$(req_hdr 'x-real-ip')" = "127.0.0.1" ] && echo "  [OK] set+占位符：X-Real-IP=127.0.0.1（覆盖伪造值）" \
  || { echo "  [FAIL] x-real-ip 应为 127.0.0.1，实际 $(req_hdr x-real-ip)" >&2; exit 1; }

[ "$(req_hdr 'x-custom')" = "added" ] && echo "  [OK] add：上游收到 X-Custom=added" \
  || { echo "  [FAIL] x-custom 应为 added，实际 $(req_hdr x-custom)" >&2; exit 1; }

[ "$(req_hdr 'x-internal')" = "null" ] && echo "  [OK] remove：上游未收到 X-Internal" \
  || { echo "  [FAIL] x-internal 应被剥离，实际 $(req_hdr x-internal)" >&2; exit 1; }

echo "== 7. 响应头改写校验 =="
resp=$(curl -sS -m 5 -D - -o /dev/null "${GATE}/rw")
powered=$(grep -i '^X-Powered-By:' <<<"$resp" | tr -d '\r' | awk '{print $2}')
xdebug=$(grep -i '^X-Debug:' <<<"$resp" | tr -d '\r' || echo "")
[ "$powered" = "conrogate" ] && echo "  [OK] set：响应头 X-Powered-By=conrogate" \
  || { echo "  [FAIL] X-Powered-By 应为 conrogate，实际 $powered" >&2; exit 1; }
[ -z "$xdebug" ] && echo "  [OK] remove：响应头 X-Debug 已被剥离" \
  || { echo "  [FAIL] X-Debug 应被剥离，实际 $xdebug" >&2; exit 1; }

echo ""
echo "== 完成 =="
echo "  上游 id=$UP_ID 路由 id=$ROUTE_ID 路径 /rw"
echo "  清理示例配置：./scripts/tests/test-plugin-header-rewrite.sh --cleanup"
