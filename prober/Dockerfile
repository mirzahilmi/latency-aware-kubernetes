FROM rust:1.88.0-alpine3.20 AS build

WORKDIR /tmp/build

RUN apk add --no-cache protoc musl-dev

COPY Cargo.lock Cargo.toml ./
RUN mkdir src && echo "// mirza ganteng" > src/lib.rs
# see https://wiki.archlinux.org/title/Rust_package_guidelines#Prepare
RUN RUSTUP_TOOLCHAIN=stable \
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"

COPY . .
# see https://wiki.archlinux.org/title/Rust_package_guidelines#Build
RUN RUSTUP_TOOLCHAIN=stable \
    CARGO_TARGET_DIR=target \
    cargo build --frozen --release --all-features

FROM alpine:3.22.1
LABEL org.opencontainers.image.source=https://github.com/mirzahilmi/latency-aware-k8s-scheduler
ARG APP_NAME=prober
COPY --from=build /tmp/build/target/release/$APP_NAME /program
CMD ["/program"]
