#!/usr/bin/env bash
set -euo pipefail

# WebSocket 接入测试：用 scripts/upstream/ws.php（Swoole）起本地 WS echo 上游，
# 注册到 conrogate（上游 + 路由），发布后经网关隧道转发回显校验。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-ws-svc.sh            # 起上游 → 注册 → 发布 → 网关 WS 回显校验
#   ./scripts/tests/test-ws-svc.sh --cleanup  # 删除示例上游与路由
#
# 可覆盖的环境变量：
#   CONROGATE_CONTROL_BASE   控制面地址，默认 http://127.0.0.1:9000/api/v1
#   CONROGATE_GATE_BASE      数据面地址，默认 http://127.0.0.1:8080
#   CONROGATE_CONTROL_AUTH_TOKEN 鉴权 token（与启动配置一致，空=无鉴权）
#   WS_PATH                  路由前缀，默认 /ws
#   WS_MESSAGE               测试消息，默认带随机后缀

BASE="${CONROGATE_CONTROL_BASE:-http://127.0.0.1:9000/api/v1}"
GATE="${CONROGATE_GATE_BASE:-http://127.0.0.1:8080}"
TOKEN="${CONROGATE_CONTROL_AUTH_TOKEN:-}"
WS_PATH="${WS_PATH:-/ws}"
UPSTREAM_NAME="ws-echo"
ROUTE_NAME="ws-echo-route"
WS_SERVER="$(cd "$(dirname "$0")" && pwd)/../upstream/ws.php"   # Swoole：服务器 + 客户端回显校验
WS_HOST="127.0.0.1"
WS_PORT=""
WS_PID=""
WS_LOG=""

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

start_ws_server() {
  WS_LOG=$(mktemp)
  php "$WS_SERVER" --host "$WS_HOST" --port 0 >"$WS_LOG" 2>&1 &
  WS_PID=$!
  for _ in $(seq 1 50); do
    WS_PORT=$(sed -n '1p' "$WS_LOG" 2>/dev/null || echo "")
    [ -n "$WS_PORT" ] && break
    kill -0 "$WS_PID" 2>/dev/null || {
      echo "  [FAIL] ws.php 启动失败：$(cat "$WS_LOG")" >&2
      exit 1
    }
    sleep 0.1
  done
  [ -n "$WS_PORT" ] || { echo "  [FAIL] ws.php 未输出端口" >&2; exit 1; }
}

stop_ws_server() {
  if [ -n "$WS_PID" ]; then
    kill "$WS_PID" 2>/dev/null || true
    wait "$WS_PID" 2>/dev/null || true
  fi
  [ -n "$WS_LOG" ] && rm -f "$WS_LOG"
  WS_PID=""
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
trap stop_ws_server EXIT

echo "== 1. 前置体检 =="
healthz=$(curl -sS -m 5 "${BASE%/api/v1}/healthz")
[ "$(jget '.code' <<<"$healthz")" = "0" ] || { echo "  [FAIL] 控制面未就绪：$healthz" >&2; exit 1; }
echo "  [OK] 控制面健康：$(jget '.data.status' <<<"$healthz")"
ready=$(curl -sS -m 5 "${GATE}/readyz")
[ "$ready" = "ready" ] || { echo "  [FAIL] 数据面未就绪：$ready" >&2; exit 1; }
echo "  [OK] 数据面就绪"

echo "== 2. 启动本地 WS echo 上游（ws.php，Swoole） =="
start_ws_server
echo "  [OK] WS 上游已启动：$WS_HOST:$WS_PORT（pid=$WS_PID）"

echo "== 3. 注册上游 =="
upstream_id=$(curl -sS "${BASE}/upstreams" ${AUTH[@]+"${AUTH[@]}"} | jget '.data.list[]|select(.name=="'"$UPSTREAM_NAME"'")|.id' 2>/dev/null | head -n1 || echo "")
if [ -n "$upstream_id" ] && [ "$upstream_id" != "null" ]; then
  # 复用上游时更新节点地址为本次 Swoole 的随机端口（旧端口已失效）
  curl -sS -X PATCH "${BASE}/upstreams/${upstream_id}" ${AUTH[@]+"${AUTH[@]}"} \
    -H 'Content-Type: application/json' \
    -d "{\"id\":$upstream_id,\"nodes\":[{\"address\":\"$WS_HOST:$WS_PORT\",\"weight\":1,\"enabled\":true}]}" >/dev/null
  echo "  [OK] 复用已有上游 id=$upstream_id 并更新节点地址为 $WS_HOST:$WS_PORT"
else
  up_resp=$(post_json "/upstreams" "{
    \"name\": \"$UPSTREAM_NAME\",
    \"algorithm\": \"round_robin\",
    \"retry_enabled\": false,
    \"nodes\": [{\"address\": \"$WS_HOST:$WS_PORT\", \"weight\": 1, \"enabled\": true}]
  }")
  upstream_id=$(need "$up_resp" "id" "上游")
  echo "  [OK] 上游创建成功 id=$upstream_id"
fi

echo "== 4. 注册路由（$WS_PATH，WebSocket 走 HTTP 升级路径） =="
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
      \"path\": {\"prefix\": \"$WS_PATH\"},
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
pub_resp=$(curl -sS -X POST "${BASE}/configs/publish?base_version=${latest}&remark=ws%20route" ${AUTH[@]+"${AUTH[@]}"})
pub_code=$(jget '.code' <<<"$pub_resp")
if [ "$pub_code" = "0" ]; then
  echo "  [OK] 发布成功 version=$(jget '.data.version' <<<"$pub_resp")"
elif [ "$pub_code" = "20007" ]; then
  echo "  [OK] 发布并发冲突（配置已由其他版本发布，热载以 DB 为准）"
else
  echo "  [FAIL] 发布失败：$pub_resp" >&2
  exit 1
fi

echo "== 6. 经网关 WebSocket 隧道转发校验 =="
# 配置热载为定时轮询（约 5s 周期），轮询等待路由生效（静默重试）
rc=1
for _ in $(seq 1 20); do
  if php "$WS_SERVER" --client "ws://${GATE#http://}${WS_PATH}" --message "ws-test-$$" >/dev/null 2>&1; then
    rc=0
    break
  fi
  sleep 1
done
if [ "$rc" = "0" ]; then
  echo "  [OK] 网关 WebSocket 隧道回显校验通过"
  php "$WS_SERVER" --client "ws://${GATE#http://}${WS_PATH}" --message "ws-echo-$$"
else
  echo "  [FAIL] 网关 WS 转发校验失败" >&2
  exit 1
fi

echo ""
echo "== 完成 =="
echo "  上游 id=$upstream_id 路由 id=$route_id 路径 $WS_PATH"
echo "  清理示例配置：./scripts/tests/test-ws-svc.sh --cleanup"
