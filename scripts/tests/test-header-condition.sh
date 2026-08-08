#!/usr/bin/env bash
set -euo pipefail

# 路由多维条件匹配测试：同一路径前缀下按 header / query 条件分流 + 优先级回落。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-header-condition.sh            # 起 3 个本地 echo 上游 → 注册路由 → 发布 → 匹配校验
#   ./scripts/tests/test-header-condition.sh --cleanup  # 删除示例上游与路由
#
# 场景（全部路由共用 /hrc 与 /hrq 前缀）：
#   /hrc  X-Tenant: alpha    → 上游 A（exact 匹配，priority 30）
#   /hrc  X-Tenant: <非空>   → 上游 B（not_empty 匹配，priority 20）
#   /hrc  无 X-Tenant        → 上游 C（无 header 条件兜底，priority 10）
#   /hrq  ?version=v2        → 上游 A（query exact 匹配，priority 30）
#   /hrq  ?version=其他/无   → 上游 C（query 兜底，priority 10）
#
# 可覆盖的环境变量：同 lib-conrogate.sh（CONROGATE_CONTROL_BASE 等）

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"

UP_A="hdr-up-a"
UP_B="hdr-up-b"
UP_C="hdr-up-c"
ROUTE_ALPHA="hdr-route-alpha"
ROUTE_NOTEMPTY="hdr-route-notempty"
ROUTE_FALLBACK="hdr-route-fallback"
ROUTE_QV2="hdr-route-qv2"
ROUTE_QFB="hdr-route-qfb"

declare -a ECHO_PIDS ECHO_LOGS
PORTS_A="" PORTS_B="" PORTS_C=""

cleanup() {
  echo "== 清理示例配置 =="
  local deleted=0
  for name in "$ROUTE_ALPHA" "$ROUTE_NOTEMPTY" "$ROUTE_FALLBACK" "$ROUTE_QV2" "$ROUTE_QFB"; do
    local id
    id=$(route_id_by_name "$name")
    if [ -n "$id" ]; then
      api DELETE "/routes/${id}" >/dev/null
      echo "  [OK] 路由 $name (id=$id) 已删除"
      deleted=1
    fi
  done
  for name in "$UP_A" "$UP_B" "$UP_C"; do
    local id
    id=$(upstream_id_by_name "$name")
    if [ -n "$id" ]; then
      api DELETE "/upstreams/${id}" >/dev/null
      echo "  [OK] 上游 $name (id=$id) 已删除"
      deleted=1
    fi
  done
  [ "$deleted" = "1" ] || echo "  [SKIP] 未找到示例配置"
  echo "  [提示] 删除需重新发布生效"
  curl -sS -X POST "${BASE}/configs/publish" ${AUTH[@]+"${AUTH[@]}"} >/dev/null 2>&1 || true
  exit 0
}

stop_echos() {
  local i
  for i in "${!ECHO_PIDS[@]}"; do
    kill "${ECHO_PIDS[$i]}" 2>/dev/null || true
  done
  for f in "${ECHO_LOGS[@]}"; do
    [ -n "$f" ] && rm -f "$f"
  done
}

[ "${1:-}" = "--cleanup" ] && cleanup
trap stop_echos EXIT

echo "== 1. 前置体检 =="
health_check

echo "== 2. 启动 3 个本地 echo 上游（A/B/C） =="
start_echo_server; PORTS_A=$ECHO_PORT; ECHO_PIDS+=("$ECHO_PID"); ECHO_LOGS+=("$ECHO_LOG")
start_echo_server; PORTS_B=$ECHO_PORT; ECHO_PIDS+=("$ECHO_PID"); ECHO_LOGS+=("$ECHO_LOG")
start_echo_server; PORTS_C=$ECHO_PORT; ECHO_PIDS+=("$ECHO_PID"); ECHO_LOGS+=("$ECHO_LOG")
echo "  [OK] 上游 A=127.0.0.1:$PORTS_A  B=127.0.0.1:$PORTS_B  C=127.0.0.1:$PORTS_C"

echo "== 3. 注册上游 =="
UPA=$(ensure_upstream "$UP_A" "127.0.0.1:$PORTS_A")
UPB=$(ensure_upstream "$UP_B" "127.0.0.1:$PORTS_B")
UPC=$(ensure_upstream "$UP_C" "127.0.0.1:$PORTS_C")
echo "  [OK] 上游 id: $UP_A=$UPA  $UP_B=$UPB  $UP_C=$UPC"

echo "== 4. 注册路由（同前缀，条件分流 + 兜底） =="
ALPHA=$(ensure_route "$ROUTE_ALPHA" '{"path":{"prefix":"/hrc"},"headers":[{"key":"X-Tenant","op":"exact","value":"alpha"}]}' "$UPA" 30)
NOTEMPTY=$(ensure_route "$ROUTE_NOTEMPTY" '{"path":{"prefix":"/hrc"},"headers":[{"key":"X-Tenant","op":"not_empty","value":""}]}' "$UPB" 20)
FALLBACK=$(ensure_route "$ROUTE_FALLBACK" '{"path":{"prefix":"/hrc"}}' "$UPC" 10)
QV2=$(ensure_route "$ROUTE_QV2" '{"path":{"prefix":"/hrq"},"query_params":[{"key":"version","op":"exact","value":"v2"}]}' "$UPA" 30)
QFB=$(ensure_route "$ROUTE_QFB" '{"path":{"prefix":"/hrq"}}' "$UPC" 10)
echo "  [OK] 路由 id: alpha=$ALPHA  notempty=$NOTEMPTY  fallback=$FALLBACK  qv2=$QV2  qfb=$QFB"

echo "== 5. 发布配置 =="
publish_config "header-condition-test"

echo "== 6. 匹配校验 =="
wait_http 200 "/hrc" >/dev/null

# 从响应头 X-Upstream-Port 反查命中的上游端口
hit_port() { # hit_port <gate-path> [curl-args...]
  curl -sS -m 5 -D - -o /dev/null "${GATE}$1" "${@:2}" | tr -d '\r' | grep -i '^x-upstream-port:' | awk '{print $2}'
}

PORT_A=$(hit_port "/hrc" -H "X-Tenant: alpha")
[ "$PORT_A" = "$PORTS_A" ] && echo "  [OK] X-Tenant=alpha  → 上游 A（exact 匹配，端口 $PORT_A）" \
  || { echo "  [FAIL] X-Tenant=alpha 应命中 A($PORTS_A)，实际 $PORT_A" >&2; exit 1; }

PORT_B=$(hit_port "/hrc" -H "X-Tenant: beta")
[ "$PORT_B" = "$PORTS_B" ] && echo "  [OK] X-Tenant=beta   → 上游 B（not_empty 匹配，端口 $PORT_B）" \
  || { echo "  [FAIL] X-Tenant=beta 应命中 B($PORTS_B)，实际 $PORT_B" >&2; exit 1; }

PORT_C=$(hit_port "/hrc")
[ "$PORT_C" = "$PORTS_C" ] && echo "  [OK] 无 X-Tenant      → 上游 C（兜底，端口 $PORT_C）" \
  || { echo "  [FAIL] 无 X-Tenant 应命中 C($PORTS_C)，实际 $PORT_C" >&2; exit 1; }

PORT_Q=$(hit_port "/hrq?version=v2")
[ "$PORT_Q" = "$PORTS_A" ] && echo "  [OK] ?version=v2      → 上游 A（query exact 匹配，端口 $PORT_Q）" \
  || { echo "  [FAIL] ?version=v2 应命中 A($PORTS_A)，实际 $PORT_Q" >&2; exit 1; }

PORT_Q1=$(hit_port "/hrq?version=v1")
[ "$PORT_Q1" = "$PORTS_C" ] && echo "  [OK] ?version=v1      → 上游 C（query 不匹配回落，端口 $PORT_Q1）" \
  || { echo "  [FAIL] ?version=v1 应命中 C($PORTS_C)，实际 $PORT_Q1" >&2; exit 1; }

PORT_QN=$(hit_port "/hrq")
[ "$PORT_QN" = "$PORTS_C" ] && echo "  [OK] 无 query         → 上游 C（兜底，端口 $PORT_QN）" \
  || { echo "  [FAIL] 无 query 应命中 C($PORTS_C)，实际 $PORT_QN" >&2; exit 1; }

echo ""
echo "== 完成 =="
echo "  路由 id: alpha=$ALPHA  notempty=$NOTEMPTY  fallback=$FALLBACK  qv2=$QV2  qfb=$QFB"
echo "  清理示例配置：./scripts/tests/test-header-condition.sh --cleanup"
