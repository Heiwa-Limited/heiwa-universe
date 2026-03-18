# Stage 1: Build
FROM rust:1.75-slim as builder
WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# Copy the entire workspace context
COPY . .

# Build the specific app
RUN cargo build --release -p iclaw-rust-limb

# Stage 2: Runtime
FROM debian:bookworm-slim
WORKDIR /app

# Install runtime dependencies (OpenSSL and Browser libs)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    # Chromium/Playwright dependencies for future browser use
    libglib2.0-0 \
    libnss3 \
    libnspr4 \
    libatk1.0-0 \
    libatk-bridge2.0-0 \
    libcups2 \
    libdrm2 \
    libxkbcommon0 \
    libxcomposite1 \
    libxdamage1 \
    libxext6 \
    libxfixes3 \
    librandr2 \
    libgbm1 \
    libpango-1.0-0 \
    libcairo2 \
    libasound2 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/iclaw-rust-limb /usr/local/bin/iclaw-rust-limb

# Start the agent
CMD ["iclaw-rust-limb"]
