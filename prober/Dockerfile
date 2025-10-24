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

FROM cgr.dev/chainguard/wolfi-base:latest
WORKDIR /bin

RUN apk add nftables
COPY --from=build /artifact/release/prober /prober

CMD ["/bin/prober"]
