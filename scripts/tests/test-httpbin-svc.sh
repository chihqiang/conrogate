#!/usr/bin/env bash
set -euo pipefail

# 接入示例：把 https://httpbin.org 注册为上游，并在网关按 /anything 前缀转发
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-httpbin-svc.sh           # 创建上游/路由并发布、验证
#   ./scripts/tests/test-httpbin-svc.sh --cleanup # 删除示例上游与路由（需 --keep-config 先创建）
#
# 可覆盖的环境变量：
#   CONROGATE_CONTROL_BASE   控制面地址，默认 http://127.0.0.1:9000/api/v1
#   CONROGATE_GATE_BASE      数据面地址，默认 http://127.0.0.1:8080
#   CONROGATE_CONTROL_AUTH_TOKEN 鉴权 token（与启动配置一致，空=无鉴权）

BASE="${CONROGATE_CONTROL_BASE:-http://127.0.0.1:9000/api/v1}"
GATE="${CONROGATE_GATE_BASE:-http://127.0.0.1:8080}"
TOKEN="${CONROGATE_CONTROL_AUTH_TOKEN:-}"
UPSTREAM_NAME="httpbin"
ROUTE_NAME="httpbin-anything"

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

echo "== 1. 前置体检 =="
healthz=$(curl -sS -m 5 "${BASE%/api/v1}/healthz")
[ "$(jget '.code' <<<"$healthz")" = "0" ] || { echo "  [FAIL] 控制面未就绪：$healthz" >&2; exit 1; }
echo "  [OK] 控制面健康：$(jget '.data.status' <<<"$healthz")"
ready=$(curl -sS -m 5 "${GATE}/readyz")
[ "$ready" = "ready" ] || { echo "  [FAIL] 数据面未就绪：$ready" >&2; exit 1; }
echo "  [OK] 数据面就绪"

echo "== 2. 创建上游 httpbin（已存在则复用） =="
upstream_id=$(curl -sS "${BASE}/upstreams" ${AUTH[@]+"${AUTH[@]}"} | jget '.data.list[]|select(.name=="'"$UPSTREAM_NAME"'")|.id' 2>/dev/null | head -n1 || echo "")
if [ -n "$upstream_id" ] && [ "$upstream_id" != "null" ]; then
  echo "  [OK] 复用已有上游 id=$upstream_id"
else
  up_resp=$(post_json "/upstreams" '{
    "name": "httpbin",
    "algorithm": "round_robin",
    "retry_enabled": true,
    "nodes": [{"address": "https://httpbin.org:443", "weight": 1, "enabled": true}]
  }')
  upstream_id=$(need "$up_resp" "id" "上游")
  echo "  [OK] 上游创建成功 id=$upstream_id"
fi

echo "== 3. 创建路由（GET /anything 前缀，已存在则复用） =="
route_id=$(curl -sS "${BASE}/routes" ${AUTH[@]+"${AUTH[@]}"} | jget '.data.list[]|select(.name=="'"$ROUTE_NAME"'")|.id' 2>/dev/null || echo "")
if [ -n "$route_id" ] && [ "$route_id" != "null" ]; then
  echo "  [OK] 复用已有路由 id=$route_id"
else
  rt_resp=$(post_json "/routes" "{
    \"name\": \"$ROUTE_NAME\",
    \"protocol\": \"http\",
    \"match_conditions\": {
      \"path\": {\"prefix\": \"/anything\"},
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

echo "== 4. 发布配置 =="
# 传入当前最新版本号作为 base_version，否则并发冲突（20007）
latest=$(curl -sS "${BASE}/configs/versions?page=1&page_size=1" ${AUTH[@]+"${AUTH[@]}"} | jget '.data.list[0].version' 2>/dev/null || echo "0")
[ -n "$latest" ] && [ "$latest" != "null" ] || latest=0
pub_resp=$(curl -sS -X POST "${BASE}/configs/publish?base_version=${latest}&remark=httpbin%20route" ${AUTH[@]+"${AUTH[@]}"})
pub_code=$(jget '.code' <<<"$pub_resp")
if [ "$pub_code" = "0" ]; then
  echo "  [OK] 发布成功 version=$(jget '.data.version' <<<"$pub_resp") (hash=$(jget '.data.content_hash' <<<"$pub_resp"))"
elif [ "$pub_code" = "20007" ]; then
  echo "  [OK] 发布并发冲突（配置已由其他版本发布，热载以 DB 为准）"
else
  echo "  [FAIL] 发布失败：$pub_resp" >&2
  exit 1
fi

echo "== 5. 通过网关验证转发 =="
# 配置热载为定时轮询（约 5s 周期），轮询等待路由生效
body=""
for _ in $(seq 1 20); do
  body=$(curl -sS -m 20 -w $'\n%{http_code}' "${GATE}/anything?foo=bar" || true)
  code=$(tail -n1 <<<"$body")
  [ "$code" = "200" ] && break
  sleep 1
done
code=$(tail -n1 <<<"$body")
[ "$code" = "200" ] || { echo "  [FAIL] 网关返回 $code：$(sed '$d' <<<"$body")" >&2; exit 1; }
json=$(sed '$d' <<<"$body")
echo "  [OK] HTTP $code，上游 echo 回显："
echo "$json" | jget '.url' 2>/dev/null
echo "$json" | jget '.args' 2>/dev/null

echo "== 6. 可选：绑定插件 =="
if [ -n "${BIND_CORS_PLUGIN:-}" ]; then
  curl -sS -X POST "${BASE}/routes/${route_id}/plugins" ${AUTH[@]+"${AUTH[@]}"} \
    -H 'Content-Type: application/json' \
    -d '{"plugin_name":"cors","config":{},"enabled":true}' >/dev/null
  curl -sS -X POST "${BASE}/configs/publish" ${AUTH[@]+"${AUTH[@]}"} >/dev/null
  echo "  [OK] cors 插件已绑定到路由 $route_id 并发布"
fi

echo ""
echo "== 完成 =="
echo "  上游 id=$upstream_id 路由 id=$route_id"
echo "  清理示例配置：./scripts/tests/test-httpbin-svc.sh --cleanup"
