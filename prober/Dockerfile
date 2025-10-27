FROM rust:1.88 AS build
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

FROM debian:bookworm-slim
WORKDIR /bin

RUN \
  apt-get update -y && \
  apt-get install -y iputils-ping nftables
COPY --from=build /artifact/release/prober /bin/prober

CMD ["/bin/prober"]
