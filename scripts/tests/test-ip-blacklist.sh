#!/usr/bin/env bash
set -euo pipefail

# 全局 IP 黑名单测试：通过 /security/ip_blacklist API 拉黑/解封，校验网关即时拦截。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-ip-blacklist.sh            # 起 echo 上游 → 路由 → 拉黑 127.0.0.1 → 验证拦截 → 解封 → 恢复
#   ./scripts/tests/test-ip-blacklist.sh --cleanup  # 删除示例配置（并确保黑名单已清除）
#
# 说明：本机测试客户端 IP 恒为 127.0.0.1，拉黑该地址即阻断全部网关流量；
#       脚本保证结束时解封，不影响后续测试。
#
# 可覆盖的环境变量：同 lib-conrogate.sh

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"

UP="blk-up"
ROUTE="blk-route"
BLOCK_IP="127.0.0.1"
BLACKLIST_ID=""

stop_all() { stop_echo_server; }

cleanup() {
  echo "== 清理示例配置 =="
  # 确保黑名单已解封（防止残留影响后续测试）
  if [ -n "$BLACKLIST_ID" ]; then
    api DELETE "/security/ip_blacklist/${BLACKLIST_ID}" >/dev/null
    echo "  [OK] 黑名单 id=$BLACKLIST_ID 已解除"
  else
    local id
    id=$(api GET "/security/ip_blacklist?keyword=$BLOCK_IP" | jget '.data.list[]?.id' 2>/dev/null | head -n1 || echo "")
    [ -n "$id" ] && [ "$id" != "null" ] && { api DELETE "/security/ip_blacklist/${id}" >/dev/null; echo "  [OK] 残留黑名单 id=$id 已解除"; }
  fi
  local deleted=0
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
ROUTE_ID=$(ensure_route "$ROUTE" '{"path":{"prefix":"/bl"}}' "$UP_ID" 10)
publish_config "blacklist-test"
wait_http 200 "/bl" >/dev/null
echo "  [OK] 上游 id=$UP_ID 路由 id=$ROUTE_ID，拉黑前网关返回 200"

echo "== 4. 拉黑 127.0.0.1 =="
resp=$(api POST "/security/ip_blacklist" "{\"ip_or_cidr\":\"$BLOCK_IP\",\"reason\":\"conrogate blacklist test\"}")
BLACKLIST_ID=$(need "$resp" "id" "黑名单条目")
echo "  [OK] 黑名单 id=$BLACKLIST_ID（$BLOCK_IP）"

echo "== 5. 等待网关生效并校验拦截 =="
wait_http 403 "/bl" >/dev/null
body=$(curl -sS -m 5 -w '\n%{http_code}' "${GATE}/bl")
code=$(tail -n1 <<<"$body")
body_code=$(sed -n '1p' <<<"$body" | jget '.code' 2>/dev/null || echo "")
[ "$code" = "403" ] && [ "$body_code" = "10003" ] && echo "  [OK] 已拦截：HTTP 403 code=10003" \
  || { echo "  [FAIL] 应 403/10003，实际 HTTP=$code body_code=$body_code" >&2; exit 1; }

echo "== 6. 解封 =="
OLD_ID=$BLACKLIST_ID
api DELETE "/security/ip_blacklist/${BLACKLIST_ID}" >/dev/null
BLACKLIST_ID=""
echo "  [OK] 黑名单 id=$OLD_ID 已解除"

echo "== 7. 等待网关生效并校验恢复 =="
wait_http 200 "/bl" >/dev/null
echo "  [OK] 解封后网关返回 200"

echo ""
echo "== 完成 =="
echo "  上游 id=$UP_ID 路由 id=$ROUTE_ID 路径 /bl"
echo "  清理示例配置：./scripts/tests/test-ip-blacklist.sh --cleanup"
