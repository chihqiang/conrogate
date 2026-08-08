#!/usr/bin/env bash
set -euo pipefail

# 限流测试：独立网关实例（不干扰 8080/9000 共享实例）启用 IP QPS 限流，
# 突发请求超过窗口容量后返回 429。
#
# 用法：
#   ./scripts/tests/test-rate-limit.sh            # 起隔离网关 + echo 上游 → 校验 429
#   ./scripts/tests/test-rate-limit.sh --cleanup  # 清理（隔离网关已随测试退出，无残留）
#
# 可覆盖的环境变量：
#   CONROGATE_RL_CTL_PORT   控制面端口（默认 9081）
#   CONROGATE_RL_GATE_PORT  数据面端口（默认 8083）
#   CONROGATE_RL_IP_QPS     单 IP QPS（默认 5）

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"

CTL_PORT="${CONROGATE_RL_CTL_PORT:-9081}"
GATE_PORT="${CONROGATE_RL_GATE_PORT:-8083}"
IP_QPS="${CONROGATE_RL_IP_QPS:-5}"
RL_DB="/tmp/conrogate-rl-test.db"
CTL_BASE="http://127.0.0.1:${CTL_PORT}/api/v1"
GATE_BASE="http://127.0.0.1:${GATE_PORT}"

PID=""
stop_all() { stop_echo_server; }

cleanup() {
  stop_all
  [ -n "$PID" ] && kill "$PID" 2>/dev/null || true
  rm -f "$RL_DB"
}

[ "${1:-}" = "--cleanup" ] && { cleanup; exit 0; }
trap cleanup EXIT

echo "== 1. 启动隔离网关实例（限流：IP_QPS=$IP_QPS） =="
BASE="$CTL_BASE"
GATE="$GATE_BASE"
AUTH=(-H "Authorization: Bearer $TOKEN")
export CONROGATE_GATE_RATE_LIMIT_ENABLED=true
export CONROGATE_GATE_RATE_LIMIT_GLOBAL_QPS=0
export CONROGATE_GATE_RATE_LIMIT_ROUTE_QPS=0
export CONROGATE_GATE_RATE_LIMIT_IP_QPS="$IP_QPS"
export CONROGATE_GATE_RATE_LIMIT_CONN_QPS=0
export CONROGATE_GATE_RATE_LIMIT_BANDWIDTH_KBPS=0
PID=$(start_isolated_gateway "$CTL_PORT" "$GATE_PORT" "$RL_DB")
echo "  [OK] 隔离网关已就绪（control=$CTL_PORT gate=$GATE_PORT pid=$PID）"

echo "== 2. 启动 echo 上游并注册到隔离网关 =="
start_echo_server
PORT=$ECHO_PORT
UP_ID=$(ensure_upstream "rl-up" "127.0.0.1:$PORT")
ROUTE_ID=$(ensure_route "rl-route" '{"path":{"prefix":"/rl"}}' "$UP_ID" 10)
publish_config "rate-limit-test"
wait_http 200 "/rl" >/dev/null
echo "  [OK] 路由生效"

echo "== 3. 突发 12 次请求（IP_QPS=$IP_QPS 窗口 1s） =="
ok=0; limited=0; body_code=""
for i in $(seq 1 12); do
  resp=$(curl -sS -m 5 -w $'\n%{http_code}' "${GATE}/rl" || true)
  code=$(tail -n1 <<<"$resp")
  if [ "$code" = "200" ]; then ok=$((ok + 1)); fi
  if [ "$code" = "429" ]; then
    limited=$((limited + 1))
    body_code=$(head -n1 <<<"$resp" | jget '.code' 2>/dev/null || echo "")
  fi
done
echo "  [INFO] 200=$ok 429=$limited"
[ "$ok" -ge 1 ] && echo "  [OK] 窗口容量内请求放行（$ok 次 200）" \
  || { echo "  [FAIL] 预期存在 200，实际全为限流" >&2; exit 1; }
[ "$limited" -ge 1 ] && echo "  [OK] 超窗请求被限流（$limited 次 429）" \
  || { echo "  [FAIL] 预期存在 429，实际 $limited" >&2; exit 1; }
[ "$body_code" = "40008" ] && echo "  [OK] 429 响应体 code=40008 (ERR_LIMITED)" \
  || { echo "  [FAIL] 429 响应体 code 应为 40008，实际 $body_code" >&2; exit 1; }

echo "== 4. 等待窗口滑动后恢复放行 =="
sleep 2
wait_http 200 "/rl" >/dev/null
echo "  [OK] 窗口滑动后恢复 200"

stop_all
echo "== 清理隔离网关 =="
stop_isolated_gateway "$PID" "$RL_DB"
echo "  [OK] 已停止并删除临时库"
