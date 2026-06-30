# Multi-stage Dockerfile for Cluster CI Cache
# Builds all binaries and packages them into a minimal image

FROM rust:1.95-slim AS builder
WORKDIR /build

# Install needed build tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build all binaries in release mode
RUN cargo build --release --bin ci-cache-server --bin ci-cache --bin ci-cache-agent

# --- Runtime image ---
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r cicache && useradd -r -g cicache -u 1000 cicache

# Copy binaries
COPY --from=builder /build/target/release/ci-cache-server /usr/local/bin/ci-cache-server
COPY --from=builder /build/target/release/ci-cache /usr/local/bin/ci-cache
COPY --from=builder /build/target/release/ci-cache-agent /usr/local/bin/ci-cache-agent

# Create data directory
RUN mkdir -p /var/lib/ci-cache && chown -R cicache:cicache /var/lib/ci-cache

USER cicache
EXPOSE 8080 8090

# Default to server
ENTRYPOINT ["ci-cache-server"]
CMD []
