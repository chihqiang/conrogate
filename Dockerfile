# ── 构建阶段 ──
FROM rust:1.88-bookworm AS builder

WORKDIR /build

COPY . .

# 缓存依赖下载；构建产物进入镜像层（供 COPY --from=builder 使用）
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked

# ── 运行阶段 ──
FROM debian:bookworm-slim

RUN apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 复制二进制
COPY --from=builder /build/target/release/conrogate /app/
COPY --from=builder /build/target/release/conrogate-gate /app/
COPY --from=builder /build/target/release/conrogate-control /app/
COPY --from=builder /build/target/release/conrogate-migrate /app/

# 暴露端口
EXPOSE 8080 9000
ENV PATH="/app:$PATH"

CMD ["/app/conrogate"]
