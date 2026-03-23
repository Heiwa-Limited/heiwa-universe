# apps/heiwa_hub/Dockerfile
# Heiwa AI Operating System — Railway Primary Plane
#
# Self-sufficient runtime: hub, CLI tools, MCP tools, state layer.
# MacBook/WSL are optional boost nodes, not requirements.

# -----------------------------------------------------
# STAGE 1: PYTHON BUILDER
# -----------------------------------------------------
FROM python:3.11-slim AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y git build-essential curl ca-certificates zstd && rm -rf /var/lib/apt/lists/*

COPY requirements.txt .

RUN python -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"
RUN pip install --no-cache-dir --upgrade pip && \
    pip install --no-cache-dir -r requirements.txt

# SpacetimeDB CLI (downloaded in builder, copied to runtime)
RUN curl -L "https://github.com/clockworklabs/SpacetimeDB/releases/download/v2.0.3/spacetime-x86_64-unknown-linux-gnu.tar.gz" -o spacetime.tar.gz && \
    tar -xzf spacetime.tar.gz && \
    mv spacetimedb-cli /usr/local/bin/spacetime && \
    mv spacetimedb-standalone /usr/local/bin/spacetimedb-standalone && \
    chmod +x /usr/local/bin/spacetime /usr/local/bin/spacetimedb-standalone && \
    rm spacetime.tar.gz

# -----------------------------------------------------
# STAGE 2: RUNTIME
# -----------------------------------------------------
FROM python:3.11-slim
WORKDIR /app

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PATH="/opt/venv/bin:/usr/local/bin:/root/.local/bin:$PATH" \
    LOG_LEVEL="INFO" \
    PYTHONPATH="/app/packages/heiwa_cli:/app/packages/heiwa_cognition:/app/packages/heiwa_sdk:/app/packages/heiwa_protocol:/app/packages/heiwa_identity:/app/packages/heiwa_ui:/app/apps" \
    NODE_OPTIONS="--max-old-space-size=256" \
    SPACETIMEDB_HOME="/app/.spacetime"

# System dependencies + git (for code freshness)
RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    git \
    gh \
    libpq-dev \
    zstd \
    gosu \
    && rm -rf /var/lib/apt/lists/*

# Non-root user for CLI tools that refuse to run as root (e.g. Claude Code)
RUN useradd -m -s /bin/bash heiwa

# Node.js 20 LTS (for CLI tools)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Copy venv from builder
COPY --from=builder /opt/venv /opt/venv

# Copy SpacetimeDB CLI and Server from builder
COPY --from=builder /usr/local/bin/spacetime /usr/local/bin/spacetime
COPY --from=builder /usr/local/bin/spacetimedb-standalone /usr/local/bin/spacetimedb-standalone

# --- Class 3 CLI Tools (cloud OAuth executors) ---
# Each is a thin HTTP client; inference runs on provider servers.
# Auth injected via Railway env vars at runtime.
RUN npm install -g @anthropic-ai/claude-code @google/gemini-cli @openai/codex @railway/cli wrangler 2>/dev/null || true

# Playwright browsers (for MCP browser automation)
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers
RUN npx playwright install chromium 2>/dev/null || true

# Cache-bust: change this value to force Railway to rebuild layers below
ARG CACHE_BUST=2026-03-22i

# --- Copy monorepo components explicitly ---
# Railway/Railpack decomposes COPY for caching; be explicit about what's needed.
COPY packages/ /app/packages/
COPY config/ /app/config/
COPY policies/ /app/policies/
COPY apps/heiwa_hub/ /app/apps/heiwa_hub/
COPY apps/heiwa_cli/scripts/agents/wrappers/ /app/apps/heiwa_cli/scripts/agents/wrappers/
COPY apps/heiwa_cli/scripts/ops/ /app/apps/heiwa_cli/scripts/ops/
COPY apps/heiwa_web/clients/web/ /app/apps/heiwa_web/clients/web/
COPY requirements.txt /app/requirements.txt
COPY conftest.py /app/conftest.py
COPY pyproject.toml /app/pyproject.toml

# Entrypoint + wrapper scripts
RUN chmod +x /app/apps/heiwa_hub/start.sh && \
    chmod +x /app/apps/heiwa_cli/scripts/agents/wrappers/*.sh && \
    chown -R heiwa:heiwa /app

# Cloud Identity
RUN echo '{"uuid": "heiwaclaw", "role": "orchestrator", "capabilities": ["routing", "discord", "execution", "mcp", "cli_tools", "openclaw"]}' > identity.json

EXPOSE 8080

CMD ["bash", "/app/apps/heiwa_hub/start.sh"]
