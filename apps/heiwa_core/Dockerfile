# apps/heiwa_core/Dockerfile
# Heiwa AI Operating System — Local-first Rust core container

# -----------------------------------------------------
# STAGE 1: RUST BUILDER
# -----------------------------------------------------
FROM rust:1.95-slim AS rust-builder
WORKDIR /app

RUN apt-get update && apt-get install -y git build-essential curl ca-certificates zstd libpq-dev pkg-config && rm -rf /var/lib/apt/lists/*

# Copy the Rust workspace. `.dockerignore` excludes build output and local
# state, while copying all workspace members keeps Cargo metadata resolvable.
COPY Cargo.toml Cargo.lock ./
COPY apps/ ./apps/
COPY crates/ ./crates/

# Build the core binary
RUN cargo build --release -p heiwa-core

# -----------------------------------------------------
# STAGE 2: RUNTIME
# -----------------------------------------------------
FROM debian:bookworm-slim
WORKDIR /app

ENV LOG_LEVEL="INFO" \
    PATH="/usr/local/bin:/root/.local/bin:$PATH"

# System dependencies (minimal for production runtime)
RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    libpq-dev \
    zstd \
    procps \
    && rm -rf /var/lib/apt/lists/*

# Copy Rust binary from builder
COPY --from=rust-builder /app/target/release/heiwa-core /usr/local/bin/heiwa-core

# Copy monorepo components (config/policies only)
COPY config/ /app/config/
COPY policies/ /app/policies/
COPY apps/heiwa_core/start.sh /app/apps/heiwa_core/start.sh

# Entrypoint setup
RUN chmod +x /app/apps/heiwa_core/start.sh

EXPOSE 8080

CMD ["bash", "/app/apps/heiwa_core/start.sh"]
