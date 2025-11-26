FROM rust:1.88 AS builder
WORKDIR /program

RUN mkdir src && touch src/main.rs
COPY Cargo.toml Cargo.lock .
RUN \
  --mount=type=cache,target=/usr/local/cargo/registry \
  cargo fetch

COPY src ./src
RUN \
  --mount=type=cache,target=/usr/local/cargo/registry \
  cargo build --release --frozen --target-dir /artifact

FROM debian:trixie-slim
WORKDIR /bin

RUN \
  apt-get update -y && \
  apt-get install -y nftables
COPY --from=builder /artifact/release/proberv2 ./mirzaganteng

# see https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry#labelling-container-images
LABEL "org.opencontainers.image.source"="https://github.com/mirzahilmi/latency-aware-kubernetes"
LABEL "org.opencontainers.image.description"="Kubernetes, pod scheduler + load balancer resource usage & network latency aware using EWMA"
LABEL "org.opencontainers.image.licenses"="AGPL-3.0"

CMD ["/bin/mirzaganteng"]
