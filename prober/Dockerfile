# syntax=docker/dockerfile:1
# see https://gist.github.com/noelbundick/6922d26667616e2ba5c3aff59f0824cd

FROM rust:1.88 AS build
WORKDIR /artifact

# Capture dependencies
RUN cargo init
COPY Cargo.toml Cargo.lock .
RUN --mount=type=cache,target=/usr/local/cargo/registry cargo build --release

COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry <<EOF
  set -e
  touch /artifact/src/main.rs
  cargo build --release
EOF

FROM debian:bookworm-slim
RUN apt-get update -y && \
  apt-get install -y iputils-ping nftables
COPY --from=build /artifact/target/release/prober /prober
# ENV RUST_LOG=prober=debug
CMD ["/prober"]
