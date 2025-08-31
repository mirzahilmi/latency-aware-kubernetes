# syntax=docker/dockerfile:1.3-labs

# see https://gist.github.com/noelbundick/6922d26667616e2ba5c3aff59f0824cd
# The above line is so we can use can use heredocs in Dockerfiles. No more && and \!
# https://www.docker.com/blog/introduction-to-heredocs-in-dockerfiles/

FROM rust:1.88 AS build

# Capture dependencies
COPY Cargo.toml Cargo.lock /app/

# This step compiles only our dependencies and saves them in a layer. This is the most impactful time savings
# Note the use of --mount=type=cache. On subsequent runs, we'll have the crates already downloaded
WORKDIR /app
RUN --mount=type=cache,target=/usr/local/cargo/registry cargo build --release

# Copy our sources
COPY . .

# A bit of magic here!
# * We're mounting that cache again to use during the build, otherwise it's not present and we'll have to download those again - bad!
# * EOF syntax is neat but not without its drawbacks. We need to `set -e`, otherwise a failing command is going to continue on
# * Rust here is a bit fiddly, so we'll touch the files (even though we copied over them) to force a new build
RUN --mount=type=cache,target=/usr/local/cargo/registry <<EOF
  set -e
  # update timestamps to force a new build
  touch /app/src/main.rs
  cargo build --release
EOF

# Again, our final image is the same - a slim base and just our app
FROM debian:buster-slim
COPY --from=build /app/target/release/prober /prober
CMD ["/prober"]
