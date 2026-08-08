#!/usr/bin/env bash
set -euo pipefail

# 自动重试测试：幂等 GET 重试直到成功、重试次数有界、非幂等请求默认不重试、
# allow_retry_non_idempotent=true 后 POST 可重试。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-retry.sh            # 起 flaky 上游 → 逐项校验
#   ./scripts/tests/test-retry.sh --cleanup  # 删除示例配置
#
# 可覆盖的环境变量：同 lib-conrogate.sh

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"

FLAKY_SERVER="$ROOT/upstream/flaky.php"

stop_all() { stop_echo_server; }

cleanup() {
  echo "== 清理示例配置 =="
  local deleted=0 id
  for r in retry-get retry-post retry-post-allow; do
    id=$(route_id_by_name "$r")
    if [ -n "$id" ]; then
      api DELETE "/routes/${id}" >/dev/null
      echo "  [OK] 路由 $r (id=$id) 已删除"
      deleted=1
    fi
  done
  for u in retry-up retry-up-post retry-up-post-allow; do
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

# 启动 flaky 实例，输出实际端口；返回 flaky 进程 PID
start_flaky() { # start_flaky <fail-first> -> <port>
  local log port
  log=$(mktemp)
  php "$FLAKY_SERVER" --host 127.0.0.1 --port 0 --fail-first "$1" >"$log" 2>&1 &
  FLAKY_PID=$!
  port=""
  for _ in $(seq 1 50); do
    port=$(sed -n '1p' "$log" 2>/dev/null || echo "")
    [ -n "$port" ] && break
    kill -0 "$FLAKY_PID" 2>/dev/null || { echo "  [FAIL] flaky.php 启动失败：$(cat "$log")" >&2; return 1; }
    sleep 0.1
  done
  rm -f "$log"
  [ -n "$port" ] || { echo "  [FAIL] flaky.php 未输出端口" >&2; return 1; }
  echo "$port"
}

# 等待 POST 请求返回预期 HTTP code（POST 专用，GET 无法命中 POST 路由）
wait_post() { # wait_post <expected-code> <gate-path>
  local expected=$1 path=$2 code=""
  for _ in $(seq 1 25); do
    code=$(curl -sS -m 5 -o /dev/null -w '%{http_code}' -X POST "${GATE}${path}" || true)
    if [ "$code" = "$expected" ]; then
      return 0
    fi
    sleep 1
  done
  echo "  [FAIL] 等待 ${path} POST 返回 $expected 超时，当前 $code" >&2
  return 1
}

echo "== 1. 前置体检 =="
health_check

echo "== 2. 场景一：幂等 GET 前两次 503，重试后成功 =="
PORT=$(start_flaky 2)
echo "  [OK] flaky 上游 127.0.0.1:$PORT（前 2 次 503）"
UP_ID=$(ensure_upstream "retry-up" "127.0.0.1:$PORT")
ROUTE_ID=$(ensure_route "retry-get" '{"path":{"prefix":"/retry1"},"methods":["GET"]}' "$UP_ID" 10)
echo "  [OK] 上游 id=$UP_ID 路由 id=$ROUTE_ID"
publish_config "retry-get-test"
wait_http 200 "/retry1" >/dev/null
echo "  [OK] GET 经自动重试后 → 200"

echo "== 3. 场景二：重试次数有界（fail-first=5 > 1+max_retries） =="
PORT5=$(start_flaky 5)
UP2_ID=$(ensure_upstream "retry-up-exhaust" "127.0.0.1:$PORT5")
ROUTE2_ID=$(ensure_route "retry-get-exhaust" '{"path":{"prefix":"/retry2"},"methods":["GET"]}' "$UP2_ID" 10)
publish_config "retry-get-exhaust-test"
wait_http 503 "/retry2" >/dev/null
echo "  [OK] 重试耗尽后保留最后一次上游响应 → 503"

echo "== 4. 场景三：非幂等 POST 默认不重试（allow_retry_non_idempotent=false） =="
PORT3=$(start_flaky 2)
UP3_ID=$(ensure_upstream "retry-up-post" "127.0.0.1:$PORT3")
ROUTE3_ID=$(ensure_route "retry-post" '{"path":{"prefix":"/retry3"},"methods":["POST"]}' "$UP3_ID" 10)
publish_config "retry-post-test"
wait_post 503 "/retry3"
echo "  [OK] POST 不重试，首次 503 即返回"

echo "== 5. 场景四：allow_retry_non_idempotent=true 后 POST 可重试 =="
PORT4=$(start_flaky 2)
UP4_ID=$(ensure_upstream "retry-up-post-allow" "127.0.0.1:$PORT4")
ROUTE4_ID=$(ensure_route "retry-post-allow" '{"path":{"prefix":"/retry4"},"methods":["POST"]}' "$UP4_ID" 10)
api PATCH "/routes/${ROUTE4_ID}" "{\"id\": $ROUTE4_ID, \"allow_retry_non_idempotent\": true}" >/dev/null
publish_config "retry-post-allow-test"
wait_post 200 "/retry4"
echo "  [OK] POST + allow_retry_non_idempotent=true 重试后 → 200"

echo ""
echo "== 完成 =="
echo "  场景验证：GET 重试成功 / 重试有界 / 非幂等默认不重试 / allow_retry_non_idempotent 生效"
echo "  清理示例配置：./scripts/tests/test-retry.sh --cleanup"
