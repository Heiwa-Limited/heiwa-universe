# apps/heiwa_shell/Dockerfile
# Heiwa local-first runtime — portable `heiwa` CLI container.
#
# Goal: ship `heiwa` as a single static-ish binary so it runs in
# CI, sandboxes, VPS relays, and operator boxes without
# pulling the whole Rust toolchain on the host.
#
# Build:
#   docker build -f apps/heiwa_shell/Dockerfile -t ghcr.io/strategizing/heiwa:dev .
# Run:
#   docker run --rm -v "$HOME/.heiwa:/root/.heiwa" ghcr.io/strategizing/heiwa:dev app runtime status --json
#
# State is mounted from the host so the container stays stateless.

# -----------------------------------------------------
# STAGE 1: RUST BUILDER
# -----------------------------------------------------
FROM rust:1.95-slim AS rust-builder
WORKDIR /build

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential pkg-config libssl-dev libdbus-1-dev protobuf-compiler ca-certificates git \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace layout. We keep the build context narrow so a `heiwa shell`
# rebuild does not invalidate cache on unrelated repo churn.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY apps/heiwa_core/ ./apps/heiwa_core/
COPY apps/heiwa_orchestrator/ ./apps/heiwa_orchestrator/
COPY apps/heiwa_shell/ ./apps/heiwa_shell/
COPY crates/ ./crates/

RUN cargo build --locked --release --bin heiwa -p heiwa-shell --features heiwa-shell/lance \
    && strip target/release/heiwa || true

# -----------------------------------------------------
# STAGE 2: RUNTIME
# -----------------------------------------------------
FROM debian:bookworm-slim
WORKDIR /app

ENV HEIWA_HOME="/root/.heiwa" \
    HEIWA_PROFILE="container" \
    PATH="/usr/local/bin:$PATH"

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /root/.heiwa/state

COPY --from=rust-builder /build/target/release/heiwa /usr/local/bin/heiwa

# Self-host friendly defaults: probe-only, no side effects until operator
# explicitly opts in by mounting credentials.
ENV HEIWA_DEFAULT_POLICY="local-only-no-side-effects"

ENTRYPOINT ["/usr/local/bin/heiwa"]
CMD ["app", "runtime", "status"]
