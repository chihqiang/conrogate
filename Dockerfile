# ── 构建阶段 ──
FROM rust:1.88-bookworm AS builder

WORKDIR /build

COPY . .

RUN cargo build --release --locked

# ── 运行阶段 ──
FROM debian:bookworm-slim

RUN apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 复制二进制（单一入口，子命令分发）
COPY --from=builder /build/target/release/conrogate /app/

# 暴露端口
EXPOSE 8080 9000
ENV PATH="/app:$PATH"

CMD ["/app/conrogate"]
