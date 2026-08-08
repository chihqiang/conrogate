#!/usr/bin/env bash
set -euo pipefail

# ip_allow_deny 插件测试：校验绑定级 IP 白/黑名单拦截（deny 优先、allow 白名单）。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-plugin-ip-allow-deny.sh            # 起 echo 上游 → 绑定插件 → 三阶段拦截校验
#   ./scripts/tests/test-plugin-ip-allow-deny.sh --cleanup  # 删除示例配置
#
# 场景（本机测试客户端 IP 均为 127.0.0.1）：
#   1) deny [127.0.0.1]                 → 命中黑名单 → 403 (code=10003)
#   2) allow [127.0.0.1]（deny 空）      → 白名单命中 → 200
#   3) allow [10.0.0.0/8]（deny 空）     → 白名单未命中 → 403 (code=10003)
#
# 可覆盖的环境变量：同 lib-conrogate.sh

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"

UP="ipc-up"
ROUTE="ipc-route"

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

# 断言请求被 403 拦截（code=10003）
assert_blocked() { # assert_blocked <阶段名>
  local phase=$1 body
  body=$(curl -sS -m 5 -w '\n%{http_code}' "${GATE}/ipc")
  local code body_code
  code=$(tail -n1 <<<"$body")
  body_code=$(sed -n '1p' <<<"$body" | jget '.code' 2>/dev/null || echo "")
  [ "$code" = "403" ] && [ "$body_code" = "10003" ] && echo "  [OK] $phase：403 code=10003" \
    || { echo "  [FAIL] $phase：应 403/10003，实际 HTTP=$code body_code=$body_code" >&2; exit 1; }
}

# 断言请求放行（200）
assert_allowed() { # assert_allowed <阶段名>
  local phase=$1 code
  code=$(curl -sS -m 5 -o /dev/null -w '%{http_code}' "${GATE}/ipc")
  [ "$code" = "200" ] && echo "  [OK] $phase：放行（200）" \
    || { echo "  [FAIL] $phase：应 200，实际 $code" >&2; exit 1; }
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
ROUTE_ID=$(ensure_route "$ROUTE" '{"path":{"prefix":"/ipc"}}' "$UP_ID" 10)
echo "  [OK] 上游 id=$UP_ID 路由 id=$ROUTE_ID"

echo "== 4. 阶段一：deny [127.0.0.1] → 403 =="
bind_plugin "$ROUTE_ID" "ip_allow_deny" '{"deny":["127.0.0.1"]}' true
publish_config "ipc-deny"
wait_http 403 "/ipc" >/dev/null
assert_blocked "deny 127.0.0.1"

echo "== 5. 阶段二：allow [127.0.0.1] → 放行 =="
bind_plugin "$ROUTE_ID" "ip_allow_deny" '{"allow":["127.0.0.1"]}' true
publish_config "ipc-allow"
wait_http 200 "/ipc" >/dev/null
assert_allowed "allow 127.0.0.1"

echo "== 6. 阶段三：allow [10.0.0.0/8] → 127.0.0.1 不在白名单 → 403 =="
bind_plugin "$ROUTE_ID" "ip_allow_deny" '{"allow":["10.0.0.0/8"]}' true
publish_config "ipc-allow-cidr"
wait_http 403 "/ipc" >/dev/null
assert_blocked "allow 10.0.0.0/8"

echo ""
echo "== 完成 =="
echo "  上游 id=$UP_ID 路由 id=$ROUTE_ID 路径 /ipc"
echo "  清理示例配置：./scripts/tests/test-plugin-ip-allow-deny.sh --cleanup"
