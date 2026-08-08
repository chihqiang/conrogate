#!/usr/bin/env bash
set -euo pipefail

# CORS 插件测试：绑定 cors 插件，校验预检(OPTIONS)拦截与响应头注入。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-plugin-cors.sh            # 起 echo 上游 → 绑定 cors → 发布 → 预检/响应头校验
#   ./scripts/tests/test-plugin-cors.sh --cleanup  # 删除示例配置
#
# 可覆盖的环境变量：同 lib-conrogate.sh

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"

UP="cors-up"
ROUTE="cors-route"
ALLOWED_ORIGIN="http://allowed.example.com"

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

echo "== 2. 启动本地 echo 上游 =="
start_echo_server
PORT=$ECHO_PORT
echo "  [OK] 上游 127.0.0.1:$PORT"

echo "== 3. 注册上游 + 路由 =="
UP_ID=$(ensure_upstream "$UP" "127.0.0.1:$PORT")
ROUTE_ID=$(ensure_route "$ROUTE" '{"path":{"prefix":"/cors"},"methods":["GET","POST","OPTIONS"]}' "$UP_ID" 10)
echo "  [OK] 上游 id=$UP_ID 路由 id=$ROUTE_ID"

echo "== 4. 绑定 cors 插件 =="
bind_plugin "$ROUTE_ID" "cors" "{
  \"allow_origins\": [\"$ALLOWED_ORIGIN\"],
  \"allow_methods\": [\"GET\",\"POST\",\"OPTIONS\"],
  \"allow_headers\": [\"Content-Type\",\"Authorization\"],
  \"max_age_seconds\": 600
}"
echo "  [OK] cors 已绑定（允许 $ALLOWED_ORIGIN）"

echo "== 5. 发布配置 =="
publish_config "cors-plugin-test"
wait_http 200 "/cors" >/dev/null
echo "  [OK] 网关路由已生效"

echo "== 6. 预检请求（OPTIONS）校验 =="
resp=$(curl -sS -m 5 -D - -o /dev/null -X OPTIONS "${GATE}/cors" \
  -H "Origin: $ALLOWED_ORIGIN" -H "Access-Control-Request-Method: POST")
code=$(awk 'NR==1{print $2}' <<<"$resp")
acao=$(grep -i '^Access-Control-Allow-Origin:' <<<"$resp" | tr -d '\r' | awk '{print $2}')
acam=$(grep -i '^Access-Control-Allow-Methods:' <<<"$resp" | tr -d '\r' | awk '{$1="";print}' | sed 's/^ //')
[ "$code" = "204" ] && echo "  [OK] 预检返回 204" \
  || { echo "  [FAIL] 预检应 204，实际 $code" >&2; exit 1; }
[ "$acao" = "$ALLOWED_ORIGIN" ] && echo "  [OK] Access-Control-Allow-Origin=$acao" \
  || { echo "  [FAIL] Allow-Origin 应为 $ALLOWED_ORIGIN，实际 $acao" >&2; exit 1; }
echo "$acam" | grep -q "POST" && echo "  [OK] Access-Control-Allow-Methods=$acam" \
  || { echo "  [FAIL] 未包含 POST：$acam" >&2; exit 1; }

echo "== 7. 白名单外 Origin 预检（不应带 CORS 头） =="
resp=$(curl -sS -m 5 -D - -o /dev/null -X OPTIONS "${GATE}/cors" \
  -H "Origin: http://evil.example.com" -H "Access-Control-Request-Method: POST")
code=$(awk 'NR==1{print $2}' <<<"$resp")
acao=$(grep -i '^Access-Control-Allow-Origin:' <<<"$resp" | tr -d '\r' | awk '{print $2}' || echo "")
[ "$code" = "204" ] && echo "  [OK] 预检返回 204" \
  || { echo "  [FAIL] 预检应 204，实际 $code" >&2; exit 1; }
[ -z "$acao" ] && echo "  [OK] 未命中白名单，无 Access-Control-Allow-Origin 头" \
  || { echo "  [FAIL] 不应出现 Allow-Origin，实际 $acao" >&2; exit 1; }

echo "== 8. 正常请求响应头注入 =="
resp=$(curl -sS -m 5 -D - -o /dev/null "${GATE}/cors" -H "Origin: $ALLOWED_ORIGIN")
code=$(awk 'NR==1{print $2}' <<<"$resp")
acao=$(grep -i '^Access-Control-Allow-Origin:' <<<"$resp" | tr -d '\r' | awk '{print $2}')
[ "$code" = "200" ] && echo "  [OK] 正常请求返回 200" \
  || { echo "  [FAIL] 应 200，实际 $code" >&2; exit 1; }
[ "$acao" = "$ALLOWED_ORIGIN" ] && echo "  [OK] 响应注入 Access-Control-Allow-Origin=$acao" \
  || { echo "  [FAIL] 应为 $ALLOWED_ORIGIN，实际 $acao" >&2; exit 1; }

echo ""
echo "== 完成 =="
echo "  上游 id=$UP_ID 路由 id=$ROUTE_ID 路径 /cors"
echo "  清理示例配置：./scripts/tests/test-plugin-cors.sh --cleanup"
