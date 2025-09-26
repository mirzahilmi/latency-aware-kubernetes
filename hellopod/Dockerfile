# syntax=docker/dockerfile:1.3-labs

FROM rust:1.88 AS builder
WORKDIR /artifact

# Capture dependencies
RUN cargo init
COPY Cargo.toml Cargo.lock .
RUN --mount=type=cache,target=/usr/local/cargo/registry cargo build --release

RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall dioxus-cli --root /.cargo -y --force
ENV PATH="/.cargo/bin:$PATH"

COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry <<EOF
  set -e
  touch /artifact/src/main.rs
  dx bundle --platform web --release
EOF

FROM debian:bookworm-slim
LABEL org.opencontainers.image.source=https://github.com/mirzahilmi/latency-aware-k8s-scheduler

ENV PORT=8080
ENV IP=0.0.0.0
EXPOSE 8080

COPY --from=builder /artifact/target/dx/hellopod/release/web/ /web
CMD ["/web/server"]
