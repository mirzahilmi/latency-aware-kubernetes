FROM --platform=$BUILDPLATFORM rust:1.92.0-alpine3.20 AS chef
WORKDIR /app
ENV PKGCONFIG_SYSROOTDIR=/
RUN apk add --no-cache musl-dev openssl-dev zig
RUN cargo install --locked cargo-zigbuild cargo-chef
RUN rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
 
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json
 
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json --release --zigbuild \
  --target x86_64-unknown-linux-musl --target aarch64-unknown-linux-musl
 
COPY . .
RUN cargo zigbuild -r \
  --target x86_64-unknown-linux-musl \
  --target aarch64-unknown-linux-musl && \
  mkdir /app/linux && \
  cp target/aarch64-unknown-linux-musl/release/prog /app/linux/arm64 && \
  cp target/x86_64-unknown-linux-musl/release/prog /app/linux/amd64
 
# shouldn't be using alpine, slow
FROM alpine:3.23.2 AS runtime
WORKDIR /app
ARG TARGETPLATFORM
COPY --from=builder /app/${TARGETPLATFORM} /app/prog
CMD "/app/prog"
