#!/usr/bin/env bash
set -euo pipefail

# 并发转发测试：本地 echo 上游，并发请求经网关转发，校验全部成功且响应一致。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-concurrency.sh            # 起 echo 上游 → 路由 → 100 并发请求校验
#   ./scripts/tests/test-concurrency.sh --cleanup  # 删除示例配置
#
# 可覆盖的环境变量：
#   CONCURRENCY_REQ      并发请求总数，默认 100
#   CONCURRENCY_PARALLEL 并行度，默认 20
#   CONCURRENCY_PATH     路由前缀，默认 /cc
#   其余同 lib-conrogate.sh

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"

UP="cc-up"
ROUTE="cc-route"
PATH_PREFIX="${CONCURRENCY_PATH:-/cc}"
TOTAL="${CONCURRENCY_REQ:-100}"
PARALLEL="${CONCURRENCY_PARALLEL:-20}"
TMP_DIR=""

stop_all() { stop_echo_server; [ -n "$TMP_DIR" ] && rm -rf "$TMP_DIR"; }

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

echo "== 2. 启动本地 echo 上游 =="
start_echo_server
PORT=$ECHO_PORT
echo "  [OK] 上游 127.0.0.1:$PORT"

echo "== 3. 注册上游 + 路由 =="
UP_ID=$(ensure_upstream "$UP" "127.0.0.1:$PORT")
ROUTE_ID=$(ensure_route "$ROUTE" "{\"path\":{\"prefix\":\"$PATH_PREFIX\"}}" "$UP_ID" 10)
publish_config "concurrency-test"
wait_http 200 "${PATH_PREFIX}/probe" >/dev/null
echo "  [OK] 上游 id=$UP_ID 路由 id=$ROUTE_ID 路径 $PATH_PREFIX"

echo "== 4. 并发请求（$TOTAL 个，并行 $PARALLEL） =="
TMP_DIR=$(mktemp -d)

# 每个请求写到独立结果文件：首行为 HTTP code，末行为 body
worker() { # worker <id>
  local id=$1 f="$TMP_DIR/$1"
  curl -sS -m 10 -o "$f.body" -w '%{http_code}' "${GATE}${PATH_PREFIX}/echo?n=$id" >"$f.code" 2>/dev/null \
    && echo "$id" >"$f.done" || true
}
export -f worker
export GATE PATH_PREFIX TMP_DIR

seq 1 "$TOTAL" | xargs -P "$PARALLEL" -I{} bash -c 'worker {}'

success=0
for f in "$TMP_DIR"/*.code; do
  code=$(cat "$f")
  if [ "$code" = "200" ]; then
    success=$((success + 1))
  fi
done
echo "  [OK] 成功 $success / $TOTAL"

[ "$success" = "$TOTAL" ] && echo "  [OK] 全部请求返回 200" \
  || { echo "  [FAIL] 仅 $success/$TOTAL 成功" >&2; exit 1; }

echo "== 5. 响应一致性抽查（任意 3 条） =="
check_body() { # check_body <id>
  local id=$1
  local path query
  path=$(jget '.path' <"$TMP_DIR/$id.body")
  query=$(jget '.query' <"$TMP_DIR/$id.body")
  [ "$path" = "${PATH_PREFIX}/echo" ] && [ "$query" = "n=$id" ] \
    || { echo "  [FAIL] body 不一致：id=$id path=$path query=$query" >&2; exit 1; }
}
for n in 1 50 100; do
  [ -f "$TMP_DIR/$n.body" ] && check_body "$n"
done
echo "  [OK] 响应内容与请求一一对应（path/query 回显正确）"

echo ""
echo "== 完成 =="
echo "  上游 id=$UP_ID 路由 id=$ROUTE_ID 路径 $PATH_PREFIX，$TOTAL 并发全部 200"
echo "  清理示例配置：./scripts/tests/test-concurrency.sh --cleanup"
