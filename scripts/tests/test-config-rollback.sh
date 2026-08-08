#!/usr/bin/env bash
set -euo pipefail

# 配置版本回滚测试：创建示例配置 → 发布 → 确认生效 → 回滚到发布前版本 → 确认配置消失。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#   且仓库至少发布过一次配置（否则无版本可回滚）。
#
# 用法：
#   ./scripts/tests/test-config-rollback.sh            # 回滚链路校验
#   ./scripts/tests/test-config-rollback.sh --cleanup  # 删除示例配置并重新发布
#
# 说明：
#   1) 记录测试开始前的最新版本 V0（快照不含测试路由）
#   2) 创建测试路由并发布（V1）→ 网关生效
#   3) 回滚到 V0 → 快照恢复，测试路由被软删，网关路由表随之移除
#   4) 校验：控制面 /routes 不再列出测试路由，网关不再 200
#
# 可覆盖的环境变量：同 lib-conrogate.sh

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"

UP="rb-up"
ROUTE="rb-route"
V0=""

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

echo "== 2. 记录当前最新配置版本 V0 =="
V0=$(api GET "/configs/versions?page=1&page_size=1" | jget '.data.list[0].version' 2>/dev/null || echo "0")
[ -n "$V0" ] && [ "$V0" != "null" ] || V0=0
[ "$V0" -ge 1 ] 2>/dev/null || { echo "  [FAIL] 无已发布版本可回滚，请先运行 ./scripts/dev-up.sh 或任一 test 脚本发布配置" >&2; exit 1; }
echo "  [OK] V0=$V0"

echo "== 3. 启动本地 echo 上游并注册 =="
start_echo_server
PORT=$ECHO_PORT
UP_ID=$(ensure_upstream "$UP" "127.0.0.1:$PORT")
ROUTE_ID=$(ensure_route "$ROUTE" '{"path":{"prefix":"/rb"}}' "$UP_ID" 10)
echo "  [OK] 上游 id=$UP_ID 路由 id=$ROUTE_ID"

echo "== 4. 发布（V1）并确认网关生效 =="
publish_config "rollback-test"
wait_http 200 "/rb" >/dev/null
echo "  [OK] 网关 /rb 返回 200"

echo "== 5. 回滚到 V0 =="
resp=$(api POST "/configs/versions/${V0}/rollback" "")
code=$(jget '.code' <<<"$resp")
[ "$code" = "0" ] || { echo "  [FAIL] 回滚失败：$resp" >&2; exit 1; }
echo "  [OK] 回滚成功，新版本 $(jget '.data.version' <<<"$resp")"

echo "== 6. 校验配置消失 =="
# 6.1 控制面：路由应不再列出（回滚快照软删该路由）
route_gone=0
for _ in $(seq 1 15); do
  if [ -z "$(route_id_by_name "$ROUTE")" ]; then
    route_gone=1
    break
  fi
  sleep 1
done
[ "$route_gone" = "1" ] && echo "  [OK] 控制面 /routes 不再列出测试路由" \
  || { echo "  [FAIL] 回滚后路由仍存在" >&2; exit 1; }

# 6.2 数据面：网关等待热载后 /rb 不应再返回 200
gate_gone=0
for _ in $(seq 1 30); do
  code=$(curl -sS -m 5 -o /dev/null -w '%{http_code}' "${GATE}/rb" || true)
  if [ "$code" != "200" ]; then
    gate_gone=1
    break
  fi
  sleep 1
done
[ "$gate_gone" = "1" ] && echo "  [OK] 网关 /rb 不再返回 200（当前 HTTP $code）" \
  || { echo "  [FAIL] 回滚后网关仍 200，路由未移除" >&2; exit 1; }

echo ""
echo "== 完成 =="
echo "  回滚链路验证通过：V0=$V0 → 发布 → 回滚 → 配置消失"
echo "  清理示例配置：./scripts/tests/test-config-rollback.sh --cleanup"
