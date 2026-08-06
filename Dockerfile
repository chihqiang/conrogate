# ── 构建阶段 ──
FROM rust:1.88-bookworm AS builder

WORKDIR /build

# 安装 OpenSSL 开发库（native-tls 依赖）
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# 复制工作空间清单（利用 Docker 层缓存：依赖不变时 cargo fetch 直接命中）
COPY Cargo.toml Cargo.lock ./
COPY conrogate-core/Cargo.toml conrogate-core/
COPY conrogate-gateway/Cargo.toml conrogate-gateway/
COPY conrogate-control-svc/Cargo.toml conrogate-control-svc/
COPY conrogate-plugin-log/Cargo.toml conrogate-plugin-log/
COPY conrogate-plugin-cors/Cargo.toml conrogate-plugin-cors/
COPY conrogate-plugin-auth/Cargo.toml conrogate-plugin-auth/
COPY conrogate-migrate/Cargo.toml conrogate-migrate/
COPY conrogate-gate/Cargo.toml conrogate-gate/
COPY conrogate-control/Cargo.toml conrogate-control/
COPY conrogate/Cargo.toml conrogate/

# 创建空桩源文件，使 cargo 能解析工作空间（仅用于 fetch 依赖，真实源码随后覆盖）
RUN for dir in conrogate-core conrogate-gateway conrogate-control-svc conrogate-plugin-log \
    conrogate-plugin-cors conrogate-plugin-auth conrogate-migrate conrogate-gate \
    conrogate-control conrogate; do mkdir -p "$dir/src"; done && \
    touch \
      conrogate-core/src/lib.rs conrogate-gateway/src/lib.rs \
      conrogate-control-svc/src/lib.rs conrogate-plugin-log/src/lib.rs \
      conrogate-plugin-cors/src/lib.rs conrogate-plugin-auth/src/lib.rs \
      conrogate-migrate/src/main.rs conrogate-gate/src/main.rs \
      conrogate-control/src/main.rs conrogate/src/main.rs

# 按 Cargo.lock 预拉依赖源码（仅依赖变化时重建此层）
RUN cargo fetch

# 复制真实源码
COPY . .

# 真正构建
RUN cargo build --release -p conrogate -p conrogate-gate -p conrogate-control -p conrogate-migrate

# ── 运行阶段 ──
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

# 创建非 root 用户
RUN useradd -r -s /bin/false conrogate && \
    mkdir -p /var/log/conrogate && \
    chown -R conrogate:conrogate /var/log/conrogate
USER conrogate
WORKDIR /app

# 复制二进制
COPY --from=builder /build/target/release/conrogate /app/
COPY --from=builder /build/target/release/conrogate-gate /app/
COPY --from=builder /build/target/release/conrogate-control /app/
COPY --from=builder /build/target/release/conrogate-migrate /app/

# 暴露端口
EXPOSE 8080 9000

# /app 进 PATH，便于 docker run <image> conrogate-gate 等切换二进制
ENV PATH="/app:$PATH"

# 默认合并模式；CMD 可被 docker run <image> <binary> 覆盖
CMD ["/app/conrogate"]
