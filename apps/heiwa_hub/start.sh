#!/bin/bash
# fleets/hub/start.sh
# Unified entrypoint for Railway deployment.

echo "[HEIWA] Initializing Cloud HQ..."
HEIWA_ENABLE_TAILSCALE="${HEIWA_ENABLE_TAILSCALE:-true}"

# 0. Tailscale is optional for Cloud HQ boot. If unavailable, continue in
# non-mesh mode so the HTTP/WebSocket control plane still comes up.
if [[ "$HEIWA_ENABLE_TAILSCALE" != "true" ]]; then
    echo "[HEIWA] Tailscale disabled for this Cloud HQ boot (HEIWA_ENABLE_TAILSCALE=false)."
else
    # Install Tailscale (Railpack fallback)
    if ! command -v tailscaled &> /dev/null; then
        echo "[HEIWA] Tailscale not found. Attempting runtime install..."
        if command -v timeout &> /dev/null; then
            timeout 30 sh -c 'curl -fsSL https://tailscale.com/install.sh | sh' || true
        elif command -v gtimeout &> /dev/null; then
            gtimeout 30 sh -c 'curl -fsSL https://tailscale.com/install.sh | sh' || true
        else
            sh -c 'curl -fsSL https://tailscale.com/install.sh | sh' || true
        fi
    fi
fi

# 1. Start the daemon in userspace mode (Critical for Railway when mesh is enabled)
export PATH=$PATH:/usr/local/bin:/root/.local/bin
TAILSCALE_READY=false

if [[ "$HEIWA_ENABLE_TAILSCALE" == "true" ]] && command -v tailscaled &> /dev/null && command -v tailscale &> /dev/null; then
    echo "[HEIWA] Starting tailscaled (Userspace Mode, Ephemeral State)..."
    # --state=mem: ensures no local state is persisted between runs.
    tailscaled --tun=userspace-networking --socket=/tmp/tailscaled.sock --state=mem: &
    TAILSCALE_READY=true
else
    echo "[HEIWA] Tailscale binaries unavailable. Proceeding without mesh."
fi

# 2. Wait for the daemon to wake up
AUTH_KEY="${TAILSCALE_AUTH_KEY:-${RAILWAY_TAILSCALE_AUTH_KEY:-}}"
TS_HOSTNAME="${TAILSCALE_HOSTNAME:-heiwa-core}"
TS_TIMEOUT_SEC="${TAILSCALE_UP_TIMEOUT_SEC:-12}"
MAX_RETRIES="${TAILSCALE_MAX_RETRIES:-5}"

run_tailscale_up() {
    if command -v timeout &>/dev/null; then
        timeout "$TS_TIMEOUT_SEC" tailscale --socket=/tmp/tailscaled.sock up --authkey="$AUTH_KEY" --hostname="$TS_HOSTNAME" --accept-dns=false --accept-routes=false --reset
    elif command -v gtimeout &>/dev/null; then
        gtimeout "$TS_TIMEOUT_SEC" tailscale --socket=/tmp/tailscaled.sock up --authkey="$AUTH_KEY" --hostname="$TS_HOSTNAME" --accept-dns=false --accept-routes=false --reset
    else
        tailscale --socket=/tmp/tailscaled.sock up --authkey="$AUTH_KEY" --hostname="$TS_HOSTNAME" --accept-dns=false --accept-routes=false --reset
    fi
}

if [[ "$TAILSCALE_READY" == "true" ]]; then
    echo "[HEIWA] Waiting for Tailscale daemon..."
    if [[ -z "$AUTH_KEY" ]]; then
        echo "[HEIWA] No TAILSCALE_AUTH_KEY/RAILWAY_TAILSCALE_AUTH_KEY provided. Skipping mesh join."
    else
        for ((ATTEMPT=1; ATTEMPT<=MAX_RETRIES; ATTEMPT++)); do
            if run_tailscale_up; then
                echo "[HEIWA] Tailscale handshake succeeded."
                break
            fi

            if [[ "$ATTEMPT" -ge "$MAX_RETRIES" ]]; then
                echo "[HEIWA] Tailscale handshake failed after $MAX_RETRIES attempts. Proceeding without mesh..."
                break
            fi

            echo "[HEIWA] Waiting for Tailscale handshake... (Attempt $ATTEMPT/$MAX_RETRIES)"
            sleep 2
        done
    fi

    TS_IP=$(tailscale --socket=/tmp/tailscaled.sock ip -4 2>/dev/null)
    if [ -n "$TS_IP" ]; then
        echo "[HEIWA] Tailscale is UP. IP: $TS_IP"
    else
        echo "[HEIWA] Tailscale is DOWN or unavailable."
    fi
else
    echo "[HEIWA] Tailscale startup skipped."
fi

# 3. Start the Ollama Micro-Brain (optional)
HEIWA_ENABLE_OLLAMA="${HEIWA_ENABLE_OLLAMA:-false}"
HEIWA_OLLAMA_PREFETCH_MODEL="${HEIWA_OLLAMA_PREFETCH_MODEL:-false}"

if [[ "$HEIWA_ENABLE_OLLAMA" == "true" ]]; then
    if command -v ollama &>/dev/null; then
        echo "[HEIWA] Starting Local Micro-Brain (Ollama)..."
        OLLAMA_HOST=0.0.0.0 ollama serve > /tmp/ollama.log 2>&1 &

        echo "[HEIWA] Waiting for Ollama to become responsive..."
        for i in {1..15}; do
            if curl -s http://127.0.0.1:11434/api/tags > /dev/null; then
                echo "[HEIWA] Ollama is UP."
                break
            fi
            sleep 2
        done

        if [[ "$HEIWA_OLLAMA_PREFETCH_MODEL" == "true" ]]; then
            echo "[HEIWA] Ensuring phi3:mini model is available locally..."
            ollama pull phi3:mini &
        else
            echo "[HEIWA] Skipping Ollama model prefetch (HEIWA_OLLAMA_PREFETCH_MODEL=false)."
        fi
    else
        echo "[HEIWA] HEIWA_ENABLE_OLLAMA=true but ollama binary is not installed. Continuing without local micro-brain."
    fi
else
    echo "[HEIWA] Ollama disabled for this Cloud HQ boot (HEIWA_ENABLE_OLLAMA=false)."
fi

# 4. Launch the Core Collective (Spine + Messenger)
export HEIWA_STATE_BACKEND="${HEIWA_STATE_BACKEND:-spacetimedb}"
export STDB_SERVER="${STDB_SERVER:-local}"
export STDB_IDENTITY="${STDB_IDENTITY:-heiwaproductiondb}"

# Pre-flight check for SpacetimeDB
if [[ "$HEIWA_STATE_BACKEND" == "spacetimedb" ]]; then
    echo "[HEIWA] Verifying SpacetimeDB availability..."
    # Local fallback start for development if STDB_SERVER=local
    if [[ "$STDB_SERVER" == "local" ]] && command -v spacetime &>/dev/null; then
        if ! curl -s http://127.0.0.1:3000/v1/health >/dev/null; then
            # Configure STDB persistent data directory (Railway volume)
            STDB_DATA_DIR="${STDB_DATA_DIR:-}"
            if [[ -n "$STDB_DATA_DIR" ]]; then
                if [[ ! -d "$STDB_DATA_DIR" ]]; then
                    echo "[HEIWA] Creating STDB data directory at $STDB_DATA_DIR..."
                    mkdir -p "$STDB_DATA_DIR"
                fi
                echo "[HEIWA] Starting local SpacetimeDB with persistent volume at $STDB_DATA_DIR..."
                spacetime start --listen-addr 127.0.0.1:3000 --data-dir "$STDB_DATA_DIR" &
            else
                echo "[HEIWA] Starting local SpacetimeDB instance..."
                spacetime start --listen-addr 127.0.0.1:3000 &
            fi
            echo "[HEIWA] Waiting for SpacetimeDB to initialize..."
            sleep 10
        fi
    fi

    # STDB module publishing: only attempt for local dev (requires Rust toolchain).
    # For maincloud/production, the module is published separately via CI or manual deploy.
    if [[ "$STDB_SERVER" == "local" ]]; then
        STDB_PROJECT_DIR="apps/heiwa_hub"
        STDB_MANIFEST_PATH="$STDB_PROJECT_DIR/spacetime.json"
        if [[ ! -f "$STDB_MANIFEST_PATH" ]]; then
            echo "[HEIWA] Missing SpacetimeDB manifest at $STDB_MANIFEST_PATH." >&2
            exit 1
        fi
        if ! command -v spacetime &>/dev/null; then
            echo "[HEIWA] spacetime CLI is required for STDB boot but is not installed." >&2
            exit 1
        fi

        echo "[HEIWA] Publishing SpacetimeDB module ($STDB_IDENTITY) from $STDB_PROJECT_DIR..."
        if ! (cd "$STDB_PROJECT_DIR" && spacetime publish --server "$STDB_SERVER" "$STDB_IDENTITY"); then
            echo "[HEIWA] STDB publish failed for $STDB_IDENTITY from $STDB_PROJECT_DIR." >&2
            exit 1
        fi
    else
        echo "[HEIWA] STDB server is '$STDB_SERVER' (remote). Skipping module publish (handled by CI)."
        # Verify connectivity to remote STDB
        if command -v spacetime &>/dev/null; then
            if spacetime sql --server "$STDB_SERVER" "$STDB_IDENTITY" "SELECT 1" 2>/dev/null; then
                echo "[HEIWA] SpacetimeDB ($STDB_SERVER/$STDB_IDENTITY) is reachable."
            else
                echo "[HEIWA] WARNING: Could not reach SpacetimeDB ($STDB_SERVER/$STDB_IDENTITY). Proceeding anyway."
            fi
        fi
    fi
fi

# Bootstrap a cloud-safe default net policy if no runtime policy is mounted.
HEIWA_HOME_DIR="${HEIWA_HOME:-/root/.heiwa}"
HEIWA_NET_POLICY_TARGET="$HEIWA_HOME_DIR/policy/internet/net_policy_v2.json"
HEIWA_NET_POLICY_BOOTSTRAP_PATH="${HEIWA_NET_POLICY_BOOTSTRAP_PATH:-/app/policies/net_policy_v2.cloud_hq.json}"
if [[ ! -f "$HEIWA_NET_POLICY_TARGET" && -f "$HEIWA_NET_POLICY_BOOTSTRAP_PATH" ]]; then
    mkdir -p "$(dirname "$HEIWA_NET_POLICY_TARGET")"
    cp "$HEIWA_NET_POLICY_BOOTSTRAP_PATH" "$HEIWA_NET_POLICY_TARGET"
    echo "[HEIWA] Bootstrapped net policy to $HEIWA_NET_POLICY_TARGET"
fi

echo "[HEIWA] Launching Core Collective..."
cd /app || exit 1

# 5. Inject CLI tool credentials from Railway env vars
# These tokens are set as Railway secrets and injected into the credential
# files/env vars that each CLI tool expects on headless Linux.

# Claude Code: uses env vars directly (no keychain on Linux)
if [[ -n "${CLAUDE_OAUTH_ACCESS_TOKEN:-}" ]]; then
    export CLAUDE_CODE_OAUTH_TOKEN="$CLAUDE_OAUTH_ACCESS_TOKEN"
    echo "[HEIWA] Claude Code OAuth token injected via env var."
fi
if [[ -n "${CLAUDE_OAUTH_REFRESH_TOKEN:-}" ]]; then
    export CLAUDE_CODE_OAUTH_REFRESH_TOKEN="$CLAUDE_OAUTH_REFRESH_TOKEN"
    export CLAUDE_CODE_OAUTH_SCOPES="openid profile email offline_access"
    echo "[HEIWA] Claude Code OAuth refresh token injected via env var."
fi

# Codex: writes ~/.codex/auth.json
if [[ -n "${CODEX_OAUTH_REFRESH_TOKEN:-}" ]]; then
    mkdir -p /root/.codex
    cat > /root/.codex/auth.json <<CODEX_AUTH
{
  "auth_mode": "chatgpt",
  "OPENAI_API_KEY": null,
  "tokens": {
    "refresh_token": "${CODEX_OAUTH_REFRESH_TOKEN}"
  }
}
CODEX_AUTH
    echo "[HEIWA] Codex auth.json written."
fi

# Gemini CLI: writes ~/.gemini/oauth_creds.json
if [[ -n "${GEMINI_OAUTH_REFRESH_TOKEN:-}" ]]; then
    mkdir -p /root/.gemini
    cat > /root/.gemini/oauth_creds.json <<GEMINI_AUTH
{
  "refresh_token": "${GEMINI_OAUTH_REFRESH_TOKEN}",
  "token_type": "Bearer",
  "scope": "https://www.googleapis.com/auth/userinfo.profile openid https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email"
}
GEMINI_AUTH
    echo "[HEIWA] Gemini oauth_creds.json written."
fi

# Verify CLI tool availability and auth
echo "[HEIWA] CLI tools:"
if command -v claude &>/dev/null; then
    CLAUDE_VER=$(claude --version 2>/dev/null | head -1)
    echo "  claude:   $CLAUDE_VER"
    if [[ -n "${CLAUDE_CODE_OAUTH_TOKEN:-}" ]]; then
        echo "  claude:   auth configured (OAuth env var)"
    else
        echo "  claude:   WARNING — no auth token (set CLAUDE_OAUTH_ACCESS_TOKEN)"
    fi
else
    echo "  claude:   not installed"
fi
if command -v codex &>/dev/null; then
    echo "  codex:    available"
    if [[ -f /root/.codex/auth.json ]]; then
        echo "  codex:    auth configured (auth.json)"
    else
        echo "  codex:    WARNING — no auth (set CODEX_OAUTH_REFRESH_TOKEN)"
    fi
else
    echo "  codex:    not installed"
fi
if command -v gemini &>/dev/null; then
    echo "  gemini:   available"
    if [[ -f /root/.gemini/oauth_creds.json ]]; then
        echo "  gemini:   auth configured (oauth_creds.json)"
    else
        echo "  gemini:   WARNING — no auth (set GEMINI_OAUTH_REFRESH_TOKEN)"
    fi
else
    echo "  gemini:   not installed"
fi


# Ensure PYTHONPATH includes all monorepo packages.
# Dockerfile ENV sets this, but Railway's startCommand override may not inherit it.
export PYTHONPATH="/app/packages/heiwa_cli:/app/packages/heiwa_cognition:/app/packages/heiwa_sdk:/app/packages/heiwa_protocol:/app/packages/heiwa_identity:/app/packages/heiwa_ui:/app/apps:${PYTHONPATH:-}"

exec python -m apps.heiwa_hub.main
