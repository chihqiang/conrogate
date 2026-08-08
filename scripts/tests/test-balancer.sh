#!/usr/bin/env bash
set -euo pipefail

# 负载均衡算法测试：加权轮询（权重比例）、一致性哈希（同源稳定）、
# 最少连接（分布到多节点）、轮询（均匀）。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-balancer.sh            # 起 3 个 echo 节点 → 逐项校验
#   ./scripts/tests/test-balancer.sh --cleanup  # 删除示例配置
#
# 可覆盖的环境变量：同 lib-conrogate.sh

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"

ECHO_PIDS=""
stop_all() {
  local pid
  for pid in $ECHO_PIDS; do
    kill "$pid" 2>/dev/null || true
  done
}

cleanup() {
  echo "== 清理示例配置 =="
  local deleted=0 id
  for r in bln-weighted bln-hash bln-least bln-rr; do
    id=$(route_id_by_name "$r")
    if [ -n "$id" ]; then
      api DELETE "/routes/${id}" >/dev/null
      echo "  [OK] 路由 $r (id=$id) 已删除"
      deleted=1
    fi
  done
  for u in bln-up-weighted bln-up-hash bln-up-least bln-up-rr; do
    id=$(upstream_id_by_name "$u")
    if [ -n "$id" ]; then
      api DELETE "/upstreams/${id}" >/dev/null
      echo "  [OK] 上游 $u (id=$id) 已删除"
      deleted=1
    fi
  done
  [ "$deleted" = "1" ] || echo "  [SKIP] 未找到示例配置"
  curl -sS -X POST "${BASE}/configs/publish" ${AUTH[@]+"${AUTH[@]}"} >/dev/null 2>&1 || true
  exit 0
}

[ "${1:-}" = "--cleanup" ] && cleanup
trap stop_all EXIT

# 创建多节点上游（存在则 PATCH 算法+节点）；成功输出上游 id
ensure_multi() { # ensure_multi <name> <algorithm> <nodes-json> -> id
  local name=$1 algo=$2 nodes=$3 id resp
  id=$(upstream_id_by_name "$name")
  if [ -n "$id" ]; then
    api PATCH "/upstreams/${id}" "{\"id\": $id, \"algorithm\": \"$algo\", \"nodes\": $nodes}" >/dev/null
    echo "$id"
    return 0
  fi
  resp=$(api POST "/upstreams" "{\"name\": \"$name\", \"algorithm\": \"$algo\", \"nodes\": $nodes}")
  need "$resp" "id" "上游 $name"
}

# 请求 N 次并统计 X-Node 响应头分布；结果存入全局 ACNT/BCNT/CCNT
node_dist() { # node_dist <n> <path>
  local n=$1 path=$2 i hdr
  ACNT=0; BCNT=0; CCNT=0
  for i in $(seq 1 "$n"); do
    hdr=$(curl -sS -m 5 -D - -o /dev/null "${GATE}${path}" | grep -i '^X-Node:' | tr -d '\r' | awk '{print $2}' || echo "?")
    case "$hdr" in
      A) ACNT=$((ACNT + 1)) ;;
      B) BCNT=$((BCNT + 1)) ;;
      C) CCNT=$((CCNT + 1)) ;;
    esac
  done
}

echo "== 1. 前置体检 =="
health_check

echo "== 2. 启动 3 个 echo 节点（X-Node 区分） =="
for node in A B C; do
  start_echo_server --add-resp-header "X-Node: $node"
  eval "${node}_PORT=$ECHO_PORT"
  ECHO_PIDS="$ECHO_PIDS $ECHO_PID"
  echo "  [OK] 节点 $node 127.0.0.1:$ECHO_PORT"
done
nodes3="[{\"address\":\"127.0.0.1:$A_PORT\",\"weight\":1,\"enabled\":true},{\"address\":\"127.0.0.1:$B_PORT\",\"weight\":1,\"enabled\":true},{\"address\":\"127.0.0.1:$C_PORT\",\"weight\":1,\"enabled\":true}]"

echo "== 3. 加权轮询：A:B = 3:1 =="
W_NODES="[{\"address\":\"127.0.0.1:$A_PORT\",\"weight\":3,\"enabled\":true},{\"address\":\"127.0.0.1:$B_PORT\",\"weight\":1,\"enabled\":true}]"
UP_ID=$(ensure_multi "bln-up-weighted" "weighted_round_robin" "$W_NODES")
ROUTE_ID=$(ensure_route "bln-weighted" '{"path":{"prefix":"/lb/w"}}' "$UP_ID" 10)
publish_config "balancer-weighted-test"
wait_http 200 "/lb/w" >/dev/null
node_dist 40 "/lb/w"
echo "  [INFO] 40 次分布 A:B = $ACNT:$BCNT"
[ "$((ACNT + BCNT))" = "40" ] && echo "  [OK] 总请求数 = 40" || { echo "  [FAIL] 请求数异常" >&2; exit 1; }
[ "$ACNT" -ge $((BCNT * 2)) ] && echo "  [OK] 权重生效（A=$ACNT >= 2*B=$BCNT）" || { echo "  [FAIL] 权重未生效：A=$ACNT B=$BCNT" >&2; exit 1; }
[ "$BCNT" -ge 3 ] && echo "  [OK] 轻节点仍有流量（B=$BCNT）" || { echo "  [FAIL] 轻节点无流量" >&2; exit 1; }

echo "== 4. 一致性哈希：同源 IP 稳定命中同一节点 =="
UP_ID=$(ensure_multi "bln-up-hash" "consistent_hash" "$nodes3")
ROUTE_ID=$(ensure_route "bln-hash" '{"path":{"prefix":"/lb/h"}}' "$UP_ID" 10)
publish_config "balancer-hash-test"
wait_http 200 "/lb/h" >/dev/null
first=""
same=1
for i in $(seq 1 10); do
  n=$(curl -sS -m 5 -D - -o /dev/null "${GATE}/lb/h" | grep -i '^X-Node:' | tr -d '\r' | awk '{print $2}')
  [ -n "$first" ] || first=$n
  [ "$n" = "$first" ] || same=0
done
[ "$same" = "1" ] && echo "  [OK] 10 次全部命中节点 $first（同一 client IP）" \
  || { echo "  [FAIL] 一致性哈希未稳定命中" >&2; exit 1; }

echo "== 5. 最少连接：并发请求分布到多节点 =="
UP_ID=$(ensure_multi "bln-up-least" "least_connections" "$nodes3")
ROUTE_ID=$(ensure_route "bln-least" '{"path":{"prefix":"/lb/l"}}' "$UP_ID" 10)
publish_config "balancer-least-test"
wait_http 200 "/lb/l" >/dev/null
OUT=$(mktemp)
export GATE LB_PATH="/lb/l"
seq 1 18 | xargs -P 6 -I{} sh -c 'curl -sS -m 5 -D - -o /dev/null "${GATE}${LB_PATH}" | grep -i "^X-Node:" | tr -d "\r" | awk "{print \$2}"' >"$OUT" 2>/dev/null || true
LA=$(grep -c '^A$' "$OUT" || echo 0); LB=$(grep -c '^B$' "$OUT" || echo 0); LC=$(grep -c '^C$' "$OUT" || echo 0)
rm -f "$OUT"
echo "  [INFO] 18 个并发请求分布 A:B:C = $LA:$LB:$LC"
[ "$LA" -ge 1 ] && [ "$LB" -ge 1 ] && [ "$LC" -ge 1 ] && echo "  [OK] 三节点均有流量（并发下计数分布）" \
  || { echo "  [FAIL] 最少连接并发分布异常：$LA/$LB/$LC" >&2; exit 1; }

echo "== 6. 轮询：12 次请求均匀分布 =="
UP_ID=$(ensure_multi "bln-up-rr" "round_robin" "$nodes3")
ROUTE_ID=$(ensure_route "bln-rr" '{"path":{"prefix":"/lb/r"}}' "$UP_ID" 10)
publish_config "balancer-rr-test"
wait_http 200 "/lb/r" >/dev/null
node_dist 12 "/lb/r"
echo "  [INFO] 12 次分布 A:B:C = $ACNT:$BCNT:$CCNT"
[ "$ACNT" -ge 2 ] && [ "$BCNT" -ge 2 ] && [ "$CCNT" -ge 2 ] && echo "  [OK] 轮询均匀" \
  || { echo "  [FAIL] 轮询分布异常：$ACNT/$BCNT/$CCNT" >&2; exit 1; }

echo ""
echo "== 完成 =="
echo "  覆盖算法：加权轮询 / 一致性哈希 / 最少连接 / 轮询"
echo "  清理示例配置：./scripts/tests/test-balancer.sh --cleanup"
