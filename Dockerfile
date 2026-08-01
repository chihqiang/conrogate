# ── 构建阶段 ──
FROM rust:1.82-bookworm AS builder

WORKDIR /build

# 安装 PostgreSQL 客户端库
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# 复制工作空间清单
COPY Cargo.toml Cargo.lock ./
COPY conrogate-contract/Cargo.toml conrogate-contract/
COPY conrogate-storage/Cargo.toml conrogate-storage/
COPY conrogate-balancer/Cargo.toml conrogate-balancer/
COPY conrogate-traffic/Cargo.toml conrogate-traffic/
COPY conrogate-plugin/Cargo.toml conrogate-plugin/
COPY conrogate-gateway/Cargo.toml conrogate-gateway/
COPY conrogate-control-svc/Cargo.toml conrogate-control-svc/
COPY conrogate-plugin-log/Cargo.toml conrogate-plugin-log/
COPY conrogate-plugin-cors/Cargo.toml conrogate-plugin-cors/
COPY conrogate-plugin-auth/Cargo.toml conrogate-plugin-auth/
COPY conrogate-migrate/Cargo.toml conrogate-migrate/
COPY conrogate-gate/Cargo.toml conrogate-gate/
COPY conrogate-control/Cargo.toml conrogate-control/
COPY conrogate/Cargo.toml conrogate/

# 预拉依赖（利用 Docker 层缓存）
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release -p conrogate 2>/dev/null || true
RUN cargo build --release -p conrogate-gate 2>/dev/null || true
RUN cargo build --release -p conrogate-control 2>/dev/null || true
RUN cargo build --release -p conrogate-migrate 2>/dev/null || true

# 复制源码
COPY . .

# 构建
RUN cargo build --release -p conrogate -p conrogate-gate -p conrogate-control -p conrogate-migrate

# ── 运行阶段 ──
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

# 创建非 root 用户
RUN useradd -r -s /bin/false conrogate
USER conrogate
WORKDIR /app

# 复制二进制
COPY --from=builder /build/target/release/conrogate /app/
COPY --from=builder /build/target/release/conrogate-gate /app/
COPY --from=builder /build/target/release/conrogate-control /app/
COPY --from=builder /build/target/release/conrogate-migrate /app/

# 暴露端口
EXPOSE 8080 9000

# 默认合并模式
ENTRYPOINT ["/app/conrogate"]
