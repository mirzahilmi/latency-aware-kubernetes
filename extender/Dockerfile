# Multi-stage build for smaller final image
FROM rust:1.82 AS builder

WORKDIR /usr/src/app

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release
RUN rm src/main.rs

# Copy actual source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage - use slim debian for smaller size
FROM debian:bookworm-slim

WORKDIR /app

# Install required runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Copy the built binary
COPY --from=builder /usr/src/app/target/release/extender ./extender

# # Copy probe data
# COPY dummy_probes.json ./
# COPY local_node.json ./

# Create non-root user for security
RUN useradd --system --uid 1001 --home-dir /app --shell /bin/false extender \
    && chown -R extender:extender /app

USER extender

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3001/healthz || exit 1

EXPOSE 3001

CMD ["./extender"]