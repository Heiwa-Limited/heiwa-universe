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

RUN apt-get update && apt-get install -y git build-essential && rm -rf /var/lib/apt/lists/*

COPY requirements.txt .

RUN python -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"
RUN pip install --no-cache-dir --upgrade pip && \
    pip install --no-cache-dir -r requirements.txt

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
    NODE_OPTIONS="--max-old-space-size=256"

# System dependencies + git (for code freshness) + Chromium (for Playwright MCP)
RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    git \
    libpq-dev \
    zstd \
    chromium \
    && rm -rf /var/lib/apt/lists/*

# Node.js 20 LTS (for CLI tools)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Copy venv from builder
COPY --from=builder /opt/venv /opt/venv

# --- Class 3 CLI Tools (cloud OAuth executors) ---
# Each is a thin HTTP client; inference runs on provider servers.
# Auth injected via Railway env vars at runtime.

# Claude Code (Anthropic) — auth via ANTHROPIC_API_KEY env var
RUN npm install -g @anthropic-ai/claude-code || true

# Gemini CLI (Google) — auth via GEMINI_API_KEY or gcloud OAuth
RUN npm install -g @google/gemini-cli || true

# Codex (OpenAI) — auth via OPENAI_API_KEY env var
RUN npm install -g @openai/codex || true

# OpenClaw (multi-provider gateway)
RUN npm install -g openclaw || true

# Playwright browsers (for MCP browser automation)
RUN npx playwright install chromium 2>/dev/null || true

# Install SpacetimeDB CLI
RUN curl -L "https://github.com/clockworklabs/SpacetimeDB/releases/download/v2.0.3/spacetime-x86_64-unknown-linux-gnu.tar.gz" -o spacetime.tar.gz && \
    tar -xzf spacetime.tar.gz && \
    mv spacetimedb-cli /usr/local/bin/spacetime && \
    chmod +x /usr/local/bin/spacetime && \
    rm spacetime.tar.gz spacetimedb-standalone

# --- Copy full monorepo (agents need codebase access) ---
# .dockerignore excludes .venv, target/, docs_and_deps/, .git/, etc.
COPY . /app/

# Entrypoint
RUN chmod +x /app/apps/heiwa_hub/start.sh

# Cloud Identity — Railway-primary orchestrator with full execution capability
RUN echo '{"uuid": "heiwa-cloud-hq", "role": "orchestrator", "capabilities": ["routing", "discord", "execution", "mcp", "cli_tools"]}' > identity.json

EXPOSE 8080

CMD ["bash", "/app/apps/heiwa_hub/start.sh"]
