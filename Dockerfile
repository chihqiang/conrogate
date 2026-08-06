# ── 构建阶段 ──
FROM rust:1.88-bookworm AS builder

WORKDIR /build

# 安装 OpenSSL 开发库（native-tls 依赖）
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# 复制工作空间清单 + 源码桩（利用 Docker 层缓存，依赖不变时跳过编译）
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

# 为每个 crate 生成空源文件，使 cargo 能解析并编译依赖
RUN for dir in conrogate-core conrogate-gateway conrogate-control-svc conrogate-plugin-log \
    conrogate-plugin-cors conrogate-plugin-auth conrogate-migrate conrogate-gate \
    conrogate-control conrogate; do \
      mkdir -p $dir/src; \
      if [ "$dir" = "conrogate-core" ]; then \
        printf 'pub mod contract; pub mod balancer; pub mod plugin; pub mod protocol; pub mod storage; pub mod traffic;\n' > $dir/src/lib.rs; \
      else \
        echo 'fn main(){}' > $dir/src/main.rs; \
      fi; \
    done

# 预拉依赖（仅依赖变化时重建此层）
RUN cargo build --release -p conrogate -p conrogate-gate -p conrogate-control -p conrogate-migrate 2>/dev/null || true

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
