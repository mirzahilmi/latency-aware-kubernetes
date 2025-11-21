FROM rust:1.88 AS builder
WORKDIR /src

RUN cargo init
COPY Cargo.toml Cargo.lock .
RUN \
  --mount=type=cache,target=/usr/local/cargo/registry \
  cargo fetch

RUN \
  --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=bind,target=. \
  cargo build --release --frozen --target-dir /artifact

FROM cgr.dev/chainguard/wolfi-base:latest
WORKDIR /bin

COPY --from=builder --chown=nonroot:nonroot /artifact/release/proberv2 ./program
USER nonroot

# see https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry#labelling-container-images
LABEL "org.opencontainers.image.source"="https://github.com/mirzahilmi/latency-aware-kubernetes"
LABEL "org.opencontainers.image.description"="Kubernetes, pod scheduler + load balancer resource usage & network latency aware using EWMA"
LABEL "org.opencontainers.image.licenses"="AGPL-3.0"

CMD ["/bin/program"]
