#!/bin/bash
# apps/heiwa_core/start.sh
# Deterministic entrypoint for Heiwa Core (Rust).

echo "[HEIWA] Initializing Core Runtime..."

# 1. Environment Defaults
export HEIWA_STATE_BACKEND="${HEIWA_STATE_BACKEND:-spacetimedb}"
export STDB_SERVER="${STDB_SERVER:-local}"
export STDB_IDENTITY="${STDB_IDENTITY:-heiwaproductiondb}"
export PATH=$PATH:/usr/local/bin:/root/.local/bin

# 2. Local State Management (Bootstrap Only)
if [[ "$HEIWA_STATE_BACKEND" == "spacetimedb" && "$STDB_SERVER" == "local" ]]; then
    echo "[HEIWA] Verifying local SpacetimeDB..."
    if command -v spacetime &>/dev/null; then
        if ! curl -s http://127.0.0.1:3000/v1/health >/dev/null; then
            echo "[HEIWA] Starting local SpacetimeDB instance..."
            STDB_DATA_DIR="${STDB_DATA_DIR:-}"
            if [[ -n "$STDB_DATA_DIR" ]]; then
                mkdir -p "$STDB_DATA_DIR"
                spacetime start --listen-addr 127.0.0.1:3000 --data-dir "$STDB_DATA_DIR" &
            else
                spacetime start --listen-addr 127.0.0.1:3000 &
            fi
            sleep 5
        fi
        
        # Publish module locally if in dev
        STDB_PROJECT_DIR="apps/heiwa_hub/spacetimedb"
        if [[ -d "$STDB_PROJECT_DIR" ]]; then
            echo "[HEIWA] Publishing local module..."
            (cd "$STDB_PROJECT_DIR" && spacetime publish --server "$STDB_SERVER" "$STDB_IDENTITY") || true
        fi
    fi
fi

# 3. Net Policy Bootstrap
HEIWA_HOME_DIR="${HEIWA_HOME:-/root/.heiwa}"
HEIWA_NET_POLICY_TARGET="$HEIWA_HOME_DIR/policy/internet/net_policy_v2.json"
HEIWA_NET_POLICY_BOOTSTRAP_PATH="${HEIWA_NET_POLICY_BOOTSTRAP_PATH:-/app/policies/net_policy_v2.cloud_hq.json}"
if [[ ! -f "$HEIWA_NET_POLICY_TARGET" && -f "$HEIWA_NET_POLICY_BOOTSTRAP_PATH" ]]; then
    mkdir -p "$(dirname "$HEIWA_NET_POLICY_TARGET")"
    cp "$HEIWA_NET_POLICY_BOOTSTRAP_PATH" "$HEIWA_NET_POLICY_TARGET"
    echo "[HEIWA] Bootstrapped net policy."
fi

# 4. Launch the Rust Core
if [[ ! -f "/usr/local/bin/heiwa-core" ]]; then
    echo "[HEIWA] ERROR: /usr/local/bin/heiwa-core not found." >&2
    exit 1
fi

echo "[HEIWA] Launching heiwa-core..."
exec /usr/local/bin/heiwa-core
