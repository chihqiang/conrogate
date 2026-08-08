#!/usr/bin/env bash
set -euo pipefail

# JWT 鉴权插件测试（HS256）：缺失/无效/过期/iss/aud 校验 + require_token 开关。
#
# 前置条件：合并模式已启动（数据面 8080 + 控制面 9000）
#   ./scripts/dev-up.sh
#
# 用法：
#   ./scripts/tests/test-plugin-auth.sh            # 起 echo 上游 → 绑定 auth → 发布 → 逐项校验
#   ./scripts/tests/test-plugin-auth.sh --cleanup  # 删除示例配置
#
# 可覆盖的环境变量：同 lib-conrogate.sh

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib-conrogate.sh"

UP="auth-up"
ROUTE="auth-route"
SECRET="test-secret"
ISSUER="conrogate-test-issuer"

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
  curl -sS -X POST "${BASE}/configs/publish" ${AUTH[@]+"${AUTH[@]}"} >/dev/null 2>&1 || true
  exit 0
}

[ "${1:-}" = "--cleanup" ] && cleanup
trap stop_all EXIT

# HS256 JWT 生成（python3 + hmac）
make_jwt() { # make_jwt <secret> <payload-json> -> token
  python3 - "$1" "$2" <<'PY'
import sys, json, base64, hmac, hashlib
secret, payload = sys.argv[1], json.loads(sys.argv[2])
def b64(b):
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode()
header = b64(json.dumps({"alg": "HS256", "typ": "JWT"}, separators=(",", ":")).encode())
body = b64(json.dumps(payload, separators=(",", ":")).encode())
sig = b64(hmac.new(secret.encode(), f"{header}.{body}".encode(), hashlib.sha256).digest())
print(f"{header}.{body}.{sig}")
PY
}

now() { python3 -c 'import time; print(int(time.time()))'; }

check_auth() { # check_auth <name> <expected-code> [curl-args...]
  local name=$1 expected=$2; shift 2
  local resp code bcode
  resp=$(curl -sS -m 5 -w $'\n%{http_code}' "${GATE}/auth" "$@" || true)
  code=$(tail -n1 <<<"$resp")
  bcode=$(head -n1 <<<"$resp" | jget '.code' 2>/dev/null || echo "")
  if [ "$code" = "$expected" ]; then
    echo "  [OK] $name → HTTP $code"
  else
    echo "  [FAIL] $name 应 $expected，实际 $code（$bcode）：$resp" >&2
    exit 1
  fi
}

echo "== 1. 前置体检 =="
health_check

echo "== 2. 启动本地 echo 上游 =="
start_echo_server
PORT=$ECHO_PORT
echo "  [OK] 上游 127.0.0.1:$PORT"

echo "== 3. 注册上游 + 路由 =="
UP_ID=$(ensure_upstream "$UP" "127.0.0.1:$PORT")
ROUTE_ID=$(ensure_route "$ROUTE" '{"path":{"prefix":"/auth"}}' "$UP_ID" 10)
echo "  [OK] 上游 id=$UP_ID 路由 id=$ROUTE_ID"

echo "== 4. 绑定 auth 插件（HS256 + issuer/audience 强制校验） =="
bind_plugin "$ROUTE_ID" "auth" "{
  \"algorithm\": \"HS256\",
  \"secret\": \"$SECRET\",
  \"issuer\": \"$ISSUER\",
  \"audience\": \"gate\",
  \"require_token\": true
}"
echo "  [OK] auth 已绑定（secret=$SECRET issuer=$ISSUER audience=gate）"

echo "== 5. 发布配置 =="
publish_config "auth-plugin-test"
wait_http 401 "/auth" >/dev/null
echo "  [OK] 网关路由已生效（无 token → 401）"

NOW=$(now)
VALID_PAYLOAD="{\"sub\":\"1001\",\"exp\":$((NOW + 3600)),\"iss\":\"$ISSUER\",\"aud\":\"gate\"}"
VALID_TOKEN=$(make_jwt "$SECRET" "$VALID_PAYLOAD")
echo "  [OK] 已生成有效 token（exp=$((NOW + 3600)) iss/aud 均匹配）"

echo "== 6. 未携带 token → 401 =="
check_auth "无 token" 401

echo "== 7. 无效签名 → 401 =="
EVIL_TOKEN=$(make_jwt "wrong-secret" "$VALID_PAYLOAD")
check_auth "错误签名" 401 -H "Authorization: Bearer $EVIL_TOKEN"

echo "== 8. 过期 token → 401 =="
EXPIRED_PAYLOAD="{\"sub\":\"1001\",\"exp\":$((NOW - 3600)),\"iss\":\"$ISSUER\"}"
EXPIRED_TOKEN=$(make_jwt "$SECRET" "$EXPIRED_PAYLOAD")
check_auth "已过期" 401 -H "Authorization: Bearer $EXPIRED_TOKEN"

echo "== 9. issuer 不匹配 → 401 =="
WRONG_ISS_PAYLOAD="{\"sub\":\"1001\",\"exp\":$((NOW + 3600)),\"iss\":\"evil-issuer\",\"aud\":\"gate\"}"
WRONG_ISS_TOKEN=$(make_jwt "$SECRET" "$WRONG_ISS_PAYLOAD")
check_auth "错误 iss" 401 -H "Authorization: Bearer $WRONG_ISS_TOKEN"

echo "== 10. audience 不匹配 → 401 =="
WRONG_AUD_PAYLOAD="{\"sub\":\"1001\",\"exp\":$((NOW + 3600)),\"iss\":\"$ISSUER\",\"aud\":\"other\"}"
WRONG_AUD_TOKEN=$(make_jwt "$SECRET" "$WRONG_AUD_PAYLOAD")
check_auth "错误 aud" 401 -H "Authorization: Bearer $WRONG_AUD_TOKEN"

echo "== 11. 有效 token → 200 =="
wait_http 200 "/auth" -H "Authorization: Bearer $VALID_TOKEN" >/dev/null
check_auth "有效 token" 200 -H "Authorization: Bearer $VALID_TOKEN"

echo "== 12. require_token=false 放行未携带 token =="
bind_plugin "$ROUTE_ID" "auth" "{
  \"algorithm\": \"HS256\",
  \"secret\": \"$SECRET\",
  \"issuer\": \"$ISSUER\",
  \"require_token\": false
}"
publish_config "auth-require-token-off"
wait_http 200 "/auth" >/dev/null
echo "  [OK] require_token=false 后无 token → 200"

echo ""
echo "== 完成 =="
echo "  上游 id=$UP_ID 路由 id=$ROUTE_ID 路径 /auth"
echo "  清理示例配置：./scripts/tests/test-plugin-auth.sh --cleanup"
