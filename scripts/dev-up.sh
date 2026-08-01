#!/bin/bash
set -euo pipefail

# ── 启动基础依赖（PostgreSQL + Redis）──
docker compose -f docker-compose.deps.yml up -d

# ── 等待健康检查通过 ──
echo "waiting for postgres..."
until docker exec conrogate-pg pg_isready -U conrogate > /dev/null 2>&1; do
  sleep 1
done
echo "postgres ready"

echo "waiting for redis..."
until docker exec conrogate-redis redis-cli ping > /dev/null 2>&1; do
  sleep 1
done
echo "redis ready"

# ── 执行数据库迁移 ──
export CONROGATE_DB_PASSWORD=conrogate_dev
cargo run -p conrogate-migrate

# ── 启动合并模式（数据面 8080 + 控制面 9000）──
export CONROGATE_NODE_AUTO_MIGRATE=false
export CONROGATE_NODE_SEED_DEMO=true
export CONROGATE_CONTROL_AUTH_TOKEN=admin:dev-token:admin
cargo run -p conrogate
