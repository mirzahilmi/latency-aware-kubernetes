FROM alpine:3.23.2 AS setup
ARG TARGETPLATFORM
RUN \
  case "$TARGETPLATFORM" in \
    "linux/amd64") echo "x86_64-unknown-linux-musl" > /target_platform ;; \
    "linux/arm64") echo "aarch64-unknown-linux-musl" > /target_platform ;; \
    *) echo "Unsupported platform: $TARGETPLATFORM" >&2; exit 1 ;; \
  esac

FROM --platform=$BUILDPLATFORM lukemathwalker/cargo-chef:0.1.73-rust-1.92.0-alpine3.21 AS chef
WORKDIR /app
COPY --from=setup /target_platform /target_platform
RUN cargo install --locked cargo-zigbuild && \
  rustup target add "$(cat /target_platform)"
RUN apk add --no-cache musl-dev zig perl make

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
WORKDIR /app
COPY --from=setup /target_platform /target_platform
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json --zigbuild \
  --target "$(cat /target_platform)"
COPY . .
RUN cargo zigbuild --release --bin proberv2 --target "$(cat /target_platform)"

FROM debian:trixie-slim AS runtime
WORKDIR /app
RUN apt-get update -y && \
  apt-get install -y --no-install-recommends nftables && \
  apt-get clean && \
  rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/*-unknown-linux-musl/release/proberv2 /usr/local/bin/app
ENTRYPOINT ["/usr/local/bin/app"]
