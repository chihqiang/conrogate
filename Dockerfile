# ── 构建阶段 ──
FROM rust:1.88-bookworm AS builder


# git 仓库地址与引用（分支 / tag / commit SHA）
ARG REPO_URL=https://github.com/chihqiang/conrogate.git
ARG REPO_REF=main
ARG GIT_DEPTH=1

WORKDIR /build
RUN apt-get update && apt-get install -y git && rm -rf /var/lib/apt/lists/*

# RUN git init -q && \
#     git remote add origin ${REPO_URL} && \
#     git fetch -q --depth=${GIT_DEPTH} origin ${REPO_REF} && \
#     git checkout -q FETCH_HEAD

COPY . .

RUN cargo build --release --locked

# ── 运行阶段 ──
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
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
