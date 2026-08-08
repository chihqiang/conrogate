#!/usr/bin/env bash
set -euo pipefail

# SSE 流式接入测试：用 scripts/upstream/sse.php（Swoole）起本地 SSE 上游，
# 注册到 conrogate（上游 + 路由），发布后经网关校验：
#   1) 正文与直连一致；2) 首字节远早于整条流结束（证明流式透传而非整体缓冲）。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-sse-svc.sh            # 起上游 → 注册 → 发布 → 网关 SSE 流式校验
#   ./scripts/tests/test-sse-svc.sh --cleanup  # 删除示例上游与路由
#
# 可覆盖的环境变量：
#   CONROGATE_CONTROL_BASE   控制面地址，默认 http://127.0.0.1:9000/api/v1
#   CONROGATE_GATE_BASE      数据面地址，默认 http://127.0.0.1:8080
#   CONROGATE_CONTROL_AUTH_TOKEN 鉴权 token（与启动配置一致，空=无鉴权）
#   SSE_PATH                 路由前缀，默认 /sse
#   SSE_COUNT                流式事件条数，默认 8
#   SSE_DELAY_MS             每条间隔毫秒数，默认 250

BASE="${CONROGATE_CONTROL_BASE:-http://127.0.0.1:9000/api/v1}"
GATE="${CONROGATE_GATE_BASE:-http://127.0.0.1:8080}"
TOKEN="${CONROGATE_CONTROL_AUTH_TOKEN:-}"
SSE_PATH="${SSE_PATH:-/sse}"
SSE_COUNT="${SSE_COUNT:-8}"
SSE_DELAY_MS="${SSE_DELAY_MS:-250}"
UPSTREAM_NAME="sse-stream"
ROUTE_NAME="sse-stream-route"
SSE_SERVER="$(cd "$(dirname "$0")" && pwd)/../upstream/sse.php"
SSE_HOST="127.0.0.1"
SSE_PORT=""
SSE_PID=""
SSE_LOG=""
DIRECT_TMP=""
GATE_TMP=""

AUTH=()
[ -n "$TOKEN" ] && AUTH=(-H "Authorization: Bearer $TOKEN")

# JSON 字段提取：从 stdin 读 JSON，参数为 jq 表达式；缺省回退 python3
jget() { # jget <jq-expr>
  if command -v jq &>/dev/null; then
    jq -r "$1"
  else
    python3 -c 'import sys,json; d=json.load(sys.stdin)
for k in sys.argv[1].lstrip(".").split("."): d=d[k]
print(d)' "$1"
  fi
}

post_json() { # post_json <path> <json-body>
  curl -sS -m 15 -X POST "${BASE}${1}" -H 'Content-Type: application/json' ${AUTH[@]+"${AUTH[@]}"} -d "$2"
}

need() { # need <json> <key> <what>
  local v
  v=$(jget ".data.${2}" <<<"$1" 2>/dev/null || echo "")
  if [ -z "$v" ] || [ "$v" = "null" ]; then
    echo "  [FAIL] $3 未创建成功：$1" >&2
    exit 1
  fi
  echo "$v"
}

start_sse_server() {
  SSE_LOG=$(mktemp)
  php "$SSE_SERVER" --host "$SSE_HOST" --port 0 >"$SSE_LOG" 2>&1 &
  SSE_PID=$!
  for _ in $(seq 1 50); do
    SSE_PORT=$(sed -n '1p' "$SSE_LOG" 2>/dev/null || echo "")
    [ -n "$SSE_PORT" ] && break
    kill -0 "$SSE_PID" 2>/dev/null || {
      echo "  [FAIL] sse.php 启动失败：$(cat "$SSE_LOG")" >&2
      exit 1
    }
    sleep 0.1
  done
  [ -n "$SSE_PORT" ] || { echo "  [FAIL] sse.php 未输出端口" >&2; exit 1; }
}

stop_sse_server() {
  if [ -n "$SSE_PID" ]; then
    kill "$SSE_PID" 2>/dev/null || true
    wait "$SSE_PID" 2>/dev/null || true
  fi
  [ -n "$SSE_LOG" ] && rm -f "$SSE_LOG"
  [ -n "$DIRECT_TMP" ] && rm -f "$DIRECT_TMP"
  [ -n "$GATE_TMP" ] && rm -f "$GATE_TMP"
  SSE_PID=""
}

cleanup() {
  echo "== 清理示例配置 =="
  local ids deleted=0
  ids=$(curl -sS "${BASE}/routes" ${AUTH[@]+"${AUTH[@]}"} | jget '.data.list[]|select(.name=="'"$ROUTE_NAME"'")|.id')
  for id in $ids; do
    curl -sS -X DELETE "${BASE}/routes/${id}" ${AUTH[@]+"${AUTH[@]}"} >/dev/null
    echo "  [OK] 路由 $ROUTE_NAME (id=$id) 已删除"
    deleted=1
  done
  ids=$(curl -sS "${BASE}/upstreams" ${AUTH[@]+"${AUTH[@]}"} | jget '.data.list[]|select(.name=="'"$UPSTREAM_NAME"'")|.id')
  for id in $ids; do
    curl -sS -X DELETE "${BASE}/upstreams/${id}" ${AUTH[@]+"${AUTH[@]}"} >/dev/null
    echo "  [OK] 上游 $UPSTREAM_NAME (id=$id) 已删除"
    deleted=1
  done
  [ "$deleted" = "1" ] || echo "  [SKIP] 未找到示例配置"
  echo "  [提示] 删除需重新发布生效"
  curl -sS -X POST "${BASE}/configs/publish" ${AUTH[@]+"${AUTH[@]}"} >/dev/null 2>&1 || true
  exit 0
}

[ "${1:-}" = "--cleanup" ] && cleanup
trap stop_sse_server EXIT

echo "== 1. 前置体检 =="
healthz=$(curl -sS -m 5 "${BASE%/api/v1}/healthz")
[ "$(jget '.code' <<<"$healthz")" = "0" ] || { echo "  [FAIL] 控制面未就绪：$healthz" >&2; exit 1; }
echo "  [OK] 控制面健康：$(jget '.data.status' <<<"$healthz")"
ready=$(curl -sS -m 5 "${GATE}/readyz")
[ "$ready" = "ready" ] || { echo "  [FAIL] 数据面未就绪：$ready" >&2; exit 1; }
echo "  [OK] 数据面就绪"

echo "== 2. 启动本地 SSE 上游（sse.php，Swoole） =="
start_sse_server
echo "  [OK] SSE 上游已启动：$SSE_HOST:$SSE_PORT（pid=$SSE_PID）"

echo "== 3. 注册上游 =="
upstream_id=$(curl -sS "${BASE}/upstreams" ${AUTH[@]+"${AUTH[@]}"} | jget '.data.list[]|select(.name=="'"$UPSTREAM_NAME"'")|.id' 2>/dev/null | head -n1 || echo "")
if [ -n "$upstream_id" ] && [ "$upstream_id" != "null" ]; then
  # 复用上游时更新节点地址为本次 Swoole 的随机端口（旧端口已失效）
  curl -sS -X PATCH "${BASE}/upstreams/${upstream_id}" ${AUTH[@]+"${AUTH[@]}"} \
    -H 'Content-Type: application/json' \
    -d "{\"id\":$upstream_id,\"nodes\":[{\"address\":\"$SSE_HOST:$SSE_PORT\",\"weight\":1,\"enabled\":true}]}" >/dev/null
  echo "  [OK] 复用已有上游 id=$upstream_id 并更新节点地址为 $SSE_HOST:$SSE_PORT"
else
  up_resp=$(post_json "/upstreams" "{
    \"name\": \"$UPSTREAM_NAME\",
    \"algorithm\": \"round_robin\",
    \"retry_enabled\": false,
    \"nodes\": [{\"address\": \"$SSE_HOST:$SSE_PORT\", \"weight\": 1, \"enabled\": true}]
  }")
  upstream_id=$(need "$up_resp" "id" "上游")
  echo "  [OK] 上游创建成功 id=$upstream_id"
fi

echo "== 4. 注册路由（$SSE_PATH，SSE 走流式透传路径） =="
route_id=$(curl -sS "${BASE}/routes" ${AUTH[@]+"${AUTH[@]}"} | jget '.data.list[]|select(.name=="'"$ROUTE_NAME"'")|.id' 2>/dev/null || echo "")
if [ -n "$route_id" ] && [ "$route_id" != "null" ]; then
  # 复用路由时确保指向当前上游（可能引用已删除的旧上游）
  curl -sS -X PATCH "${BASE}/routes/${route_id}" ${AUTH[@]+"${AUTH[@]}"} \
    -H 'Content-Type: application/json' \
    -d "{\"id\":$route_id,\"upstream_id\":$upstream_id}" >/dev/null
  echo "  [OK] 复用已有路由 id=$route_id 并指向上游 $upstream_id"
else
  rt_resp=$(post_json "/routes" "{
    \"name\": \"$ROUTE_NAME\",
    \"protocol\": \"http\",
    \"match_conditions\": {
      \"path\": {\"prefix\": \"$SSE_PATH\"},
      \"methods\": [\"GET\"],
      \"headers\": [],
      \"query_params\": []
    },
    \"upstream_id\": $upstream_id,
    \"priority\": 20,
    \"enabled\": true
  }")
  route_id=$(need "$rt_resp" "id" "路由")
  echo "  [OK] 路由创建成功 id=$route_id"
fi

echo "== 5. 发布配置 =="
# 传入当前最新版本号作为 base_version，否则并发冲突（20007）
latest=$(curl -sS "${BASE}/configs/versions?page=1&page_size=1" ${AUTH[@]+"${AUTH[@]}"} | jget '.data.list[0].version' 2>/dev/null || echo "0")
[ -n "$latest" ] && [ "$latest" != "null" ] || latest=0
pub_resp=$(curl -sS -X POST "${BASE}/configs/publish?base_version=${latest}&remark=sse%20route" ${AUTH[@]+"${AUTH[@]}"})
pub_code=$(jget '.code' <<<"$pub_resp")
if [ "$pub_code" = "0" ]; then
  echo "  [OK] 发布成功 version=$(jget '.data.version' <<<"$pub_resp")"
elif [ "$pub_code" = "20007" ]; then
  echo "  [OK] 发布并发冲突（配置已由其他版本发布，热载以 DB 为准）"
else
  echo "  [FAIL] 发布失败：$pub_resp" >&2
  exit 1
fi

echo "== 6. 经网关 SSE 流式转发校验 =="
# 配置热载为定时轮询（约 5s 周期），用瞬时探针（count=1）等待路由生效
probe_ok=0
for _ in $(seq 1 20); do
  code=$(curl -sN -m 5 -o /dev/null -w '%{http_code}' "${GATE}${SSE_PATH}?count=1&delay_ms=0" || true)
  if [ "$code" = "200" ]; then
    probe_ok=1
    break
  fi
  sleep 1
done
[ "$probe_ok" = "1" ] || { echo "  [FAIL] 网关路由未生效：HTTP $code" >&2; exit 1; }
echo "  [OK] 网关路由已生效"

# 直连基准 vs 网关：正文一致 + 首字节时间（流式断言）
DIRECT_TMP=$(mktemp)
GATE_TMP=$(mktemp)
direct_meta=$(curl -sN -m 20 -o "$DIRECT_TMP" -w '%{http_code} %{time_starttransfer} %{time_total}' \
  "http://${SSE_HOST}:${SSE_PORT}${SSE_PATH}?count=${SSE_COUNT}&delay_ms=${SSE_DELAY_MS}")
read -r d_code d_ttfb d_total <<<"$direct_meta"
gate_meta=$(curl -sN -m 20 -o "$GATE_TMP" -w '%{http_code} %{time_starttransfer} %{time_total}' \
  "${GATE}${SSE_PATH}?count=${SSE_COUNT}&delay_ms=${SSE_DELAY_MS}")
read -r g_code g_ttfb g_total <<<"$gate_meta"

[ "$d_code" = "200" ] || { echo "  [FAIL] 直连上游返回 $d_code" >&2; exit 1; }
[ "$g_code" = "200" ] || { echo "  [FAIL] 网关返回 $g_code" >&2; exit 1; }

if diff -q "$DIRECT_TMP" "$GATE_TMP" >/dev/null; then
  echo "  [OK] 网关 SSE 正文与直连一致（$(wc -c <"$GATE_TMP") bytes，${SSE_COUNT} 条事件）"
else
  echo "  [FAIL] 网关正文与直连不一致" >&2
  exit 1
fi

# 流式断言：首字节应远早于整条流结束（若整体缓冲，ttfb 会接近 total）
if awk "BEGIN{exit !($g_ttfb < ($g_total - 0.5))}"; then
  echo "  [OK] 流式透传：ttfb=${g_ttfb}s，总时长=${g_total}s（直连 ttfb=${d_ttfb}s / total=${d_total}s）"
else
  echo "  [FAIL] 疑似整体缓冲：ttfb=${g_ttfb}s 接近 total=${g_total}s" >&2
  exit 1
fi

echo ""
echo "== 完成 =="
echo "  上游 id=$upstream_id 路由 id=$route_id 路径 $SSE_PATH"
echo "  清理示例配置：./scripts/tests/test-sse-svc.sh --cleanup"
