#!/usr/bin/env bash
# 供 scripts/tests/test-*.sh 引用的公共函数库。
#
# 使用约定：
#   脚本开头 `set -euo pipefail` 后 `source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"`
#   需要 echo.php（scripts/upstream/）的脚本调用 start_echo_server / stop_echo_server。
#
# 可覆盖的环境变量：
#   CONROGATE_CONTROL_BASE   控制面地址，默认 http://127.0.0.1:9000/api/v1
#   CONROGATE_GATE_BASE      数据面地址，默认 http://127.0.0.1:8080
#   CONROGATE_CONTROL_AUTH_TOKEN 鉴权 token（与启动配置一致，空=无鉴权）

BASE="${CONROGATE_CONTROL_BASE:-http://127.0.0.1:9000/api/v1}"
GATE="${CONROGATE_GATE_BASE:-http://127.0.0.1:8080}"
TOKEN="${CONROGATE_CONTROL_AUTH_TOKEN:-}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ECHO_SERVER="$ROOT/upstream/echo.php"

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

# HTTP 方法调用控制面（-m 30），输出响应体
api() { # api <method> <path> [json-body]
  local method=$1 path=$2 body=${3:-}
  if [ -n "$body" ]; then
    curl -sS -m 30 -X "$method" "${BASE}${path}" -H 'Content-Type: application/json' \
      ${AUTH[@]+"${AUTH[@]}"} -d "$body"
  else
    curl -sS -m 30 -X "$method" "${BASE}${path}" ${AUTH[@]+"${AUTH[@]}"}
  fi
}

# 提取字段并校验非空
need() { # need <json> <key-path> <what>
  local v
  v=$(jget ".data.${2}" <<<"$1" 2>/dev/null || echo "")
  if [ -z "$v" ] || [ "$v" = "null" ]; then
    echo "  [FAIL] $3 未创建成功：$1" >&2
    exit 1
  fi
  echo "$v"
}

# 发布配置（带 base_version 防 20007 并发冲突；冲突视为成功，热载以 DB 为准）
publish_config() { # publish_config [remark]
  local remark="${1:-test}"
  local latest
  latest=$(api GET "/configs/versions?page=1&page_size=1" | jget '.data.list[0].version' 2>/dev/null || echo "0")
  [ -n "$latest" ] && [ "$latest" != "null" ] || latest=0
  local resp code
  resp=$(curl -sS -m 30 -X POST "${BASE}/configs/publish?base_version=${latest}&remark=${remark}" \
    ${AUTH[@]+"${AUTH[@]}"})
  code=$(jget '.code' <<<"$resp")
  if [ "$code" = "0" ]; then
    echo "  [OK] 发布成功 version=$(jget '.data.version' <<<"$resp")"
  elif [ "$code" = "20007" ]; then
    echo "  [OK] 发布并发冲突（配置已由其他版本发布，热载以 DB 为准）"
  else
    echo "  [FAIL] 发布失败：$resp" >&2
    exit 1
  fi
}

# 等待网关返回预期 HTTP code（热载约 5s 轮询，默认重试 25s）
wait_http() { # wait_http <expected-code> <gate-path> [curl-args...]
  local expected=$1 path=$2; shift 2
  local code=""
  for _ in $(seq 1 25); do
    code=$(curl -sS -m 5 -o /dev/null -w '%{http_code}' "${GATE}${path}" "$@" || true)
    if [ "$code" = "$expected" ]; then
      return 0
    fi
    sleep 1
  done
  echo "  [FAIL] 等待 ${path} 返回 $expected 超时，当前 $code" >&2
  return 1
}

# 查找指定名称的路由 id（不存在输出空串）
route_id_by_name() { # route_id_by_name <name>
  api GET "/routes?page_size=500" | jget '.data.list[]|select(.name=="'"$1"'")|.id' 2>/dev/null | head -n1
}

# 查找指定名称的上游 id（不存在输出空串）
upstream_id_by_name() { # upstream_id_by_name <name>
  api GET "/upstreams?page_size=500" | jget '.data.list[]|select(.name=="'"$1"'")|.id' 2>/dev/null | head -n1
}

# 启动 echo.php 实例；成功输出实际端口到 stdout
start_echo_server() { # start_echo_server -> <port>；实例变量存于全局（ECHO_PID/ECHO_LOG/ECHO_PORT）
  ECHO_LOG=$(mktemp)
  php "$ECHO_SERVER" --host 127.0.0.1 --port 0 "$@" >"$ECHO_LOG" 2>&1 &
  ECHO_PID=$!
  ECHO_PORT=""
  for _ in $(seq 1 50); do
    ECHO_PORT=$(sed -n '1p' "$ECHO_LOG" 2>/dev/null || echo "")
    [ -n "$ECHO_PORT" ] && break
    kill -0 "$ECHO_PID" 2>/dev/null || {
      echo "  [FAIL] echo.php 启动失败：$(cat "$ECHO_LOG")" >&2
      return 1
    }
    sleep 0.1
  done
  [ -n "$ECHO_PORT" ] || { echo "  [FAIL] echo.php 未输出端口" >&2; return 1; }
  echo "$ECHO_PORT"
}

# 停止 echo.php 实例并清理日志
stop_echo_server() {
  if [ -n "${ECHO_PID:-}" ]; then
    kill "$ECHO_PID" 2>/dev/null || true
    wait "$ECHO_PID" 2>/dev/null || true
  fi
  [ -n "${ECHO_LOG:-}" ] && rm -f "$ECHO_LOG"
  ECHO_PID=""
  ECHO_LOG=""
  ECHO_PORT=""
}

# 创建上游（不存在时）；成功输出上游 id
ensure_upstream() { # ensure_upstream <name> <address>
  local name=$1 addr=$2 id resp
  id=$(upstream_id_by_name "$name")
  if [ -n "$id" ]; then
    # 复用上游时更新节点地址（旧的随机端口可能已失效）
    api PATCH "/upstreams/${id}" "{
      \"id\": $id,
      \"nodes\": [{\"address\": \"$addr\", \"weight\": 1, \"enabled\": true}]
    }" >/dev/null
    echo "$id"
    return 0
  fi
  resp=$(api POST "/upstreams" "{
    \"name\": \"$name\",
    \"algorithm\": \"round_robin\",
    \"retry_enabled\": false,
    \"nodes\": [{\"address\": \"$addr\", \"weight\": 1, \"enabled\": true}]
  }")
  id=$(need "$resp" "id" "上游 $name")
  echo "$id"
}

# 创建路由（不存在时）；成功输出路由 id
ensure_route() { # ensure_route <name> <match-conditions-json> <upstream-id> [priority]
  local name=$1 cond=$2 uid=$3 prio=${4:-10} id resp
  id=$(route_id_by_name "$name")
  if [ -n "$id" ]; then
    echo "$id"
    return 0
  fi
  resp=$(api POST "/routes" "{
    \"name\": \"$name\",
    \"protocol\": \"http\",
    \"match_conditions\": $cond,
    \"upstream_id\": $uid,
    \"priority\": $prio,
    \"enabled\": true
  }")
  id=$(need "$resp" "id" "路由 $name")
  echo "$id"
}

# 绑定插件到路由（存在则更新配置）；成功输出插件绑定数
bind_plugin() { # bind_plugin <route-id> <plugin-name> <config-json> [blocking]
  local rid=$1 pname=$2 pconf=$3 blocking=${4:-false} existing
  existing=$(api GET "/routes/${rid}/plugins" | jget ".data[]|select(.plugin_name==\"$pname\")|.plugin_name" 2>/dev/null | head -n1)
  if [ -n "$existing" ]; then
    api PUT "/routes/${rid}/plugins/${pname}" "{
      \"config\": $pconf, \"blocking\": $blocking, \"enabled\": true
    }" >/dev/null
  else
    api POST "/routes/${rid}/plugins" "{
      \"plugin_name\": \"$pname\",
      \"config\": $pconf,
      \"order\": 0,
      \"blocking\": $blocking,
      \"enabled\": true
    }" >/dev/null
  fi
}

# 健康检查（控制面 + 数据面）
health_check() {
  local healthz ready
  healthz=$(curl -sS -m 5 "${BASE%/api/v1}/healthz")
  [ "$(jget '.code' <<<"$healthz")" = "0" ] || { echo "  [FAIL] 控制面未就绪：$healthz" >&2; exit 1; }
  ready=$(curl -sS -m 5 "${GATE}/readyz")
  [ "$ready" = "ready" ] || { echo "  [FAIL] 数据面未就绪：$ready" >&2; exit 1; }
  echo "  [OK] 控制面 + 数据面均就绪"
}
