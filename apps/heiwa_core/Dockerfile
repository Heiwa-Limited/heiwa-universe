# apps/heiwa_core/Dockerfile
# Heiwa AI Operating System — Railway Primary Plane (Rust Core)

# -----------------------------------------------------
# STAGE 1: RUST BUILDER
# -----------------------------------------------------
FROM rust:1.75-slim AS rust-builder
WORKDIR /app

RUN apt-get update && apt-get install -y git build-essential curl ca-certificates zstd libpq-dev pkg-config && rm -rf /var/lib/apt/lists/*

# Copy workspace configuration and core components
COPY Cargo.toml Cargo.lock ./
COPY apps/heiwa_core/ ./apps/heiwa_core/
COPY apps/heiwa_hub/spacetimedb/ ./apps/heiwa_hub/spacetimedb/
COPY packages/heiwa_bindings/rust/ ./packages/heiwa_bindings/rust/

# Build the core binary
RUN cargo build --release -p heiwa-core

# -----------------------------------------------------
# STAGE 2: RUNTIME
# -----------------------------------------------------
FROM debian:bookworm-slim
WORKDIR /app

ENV LOG_LEVEL="INFO" \
    NODE_OPTIONS="--max-old-space-size=256" \
    SPACETIMEDB_HOME="/app/.spacetime" \
    PATH="/usr/local/bin:/root/.local/bin:$PATH"

# System dependencies
RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    git \
    gh \
    libpq-dev \
    zstd \
    gosu \
    procps \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN useradd -m -s /bin/bash heiwa

# Node.js 20 LTS
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# SpacetimeDB CLI (direct download)
RUN curl -L "https://github.com/clockworklabs/SpacetimeDB/releases/download/v2.0.3/spacetime-x86_64-unknown-linux-gnu.tar.gz" -o spacetime.tar.gz && \
    tar -xzf spacetime.tar.gz && \
    mv spacetimedb-cli /usr/local/bin/spacetime && \
    mv spacetimedb-standalone /usr/local/bin/spacetimedb-standalone && \
    chmod +x /usr/local/bin/spacetime /usr/local/bin/spacetimedb-standalone && \
    rm spacetime.tar.gz

# --- Class 3 CLI Tools ---
RUN npm install -g @anthropic-ai/claude-code @google/gemini-cli @openai/codex @railway/cli wrangler 2>/dev/null || true

# Copy Rust binary from builder
COPY --from=rust-builder /app/target/release/heiwa-core /usr/local/bin/heiwa-core

# Copy monorepo components
COPY packages/ /app/packages/
COPY config/ /app/config/
COPY policies/ /app/policies/
COPY apps/heiwa_core/start.sh /app/apps/heiwa_core/start.sh
COPY apps/heiwa_hub/spacetimedb/ /app/apps/heiwa_hub/spacetimedb/

# Entrypoint setup
RUN chmod +x /app/apps/heiwa_core/start.sh && \
    chown -R heiwa:heiwa /app

# Cloud Identity
RUN echo '{"uuid": "heiwaclaw", "role": "core", "capabilities": ["routing", "discord", "execution", "mcp", "cli_tools", "openclaw"]}' > identity.json

EXPOSE 8080

CMD ["bash", "/app/apps/heiwa_core/start.sh"]
