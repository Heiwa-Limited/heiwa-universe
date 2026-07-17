#!/bin/bash
# apps/heiwa_core/start.sh
# Deterministic entrypoint for Heiwa Core (Rust).

echo "[HEIWA] Initializing Core Runtime..."

# 1. Environment Defaults
export HEIWA_STATE_BACKEND="${HEIWA_STATE_BACKEND:-local-jsonl}"
export PATH=$PATH:/usr/local/bin:/root/.local/bin

# 2. Net Policy Bootstrap
HEIWA_HOME_DIR="${HEIWA_HOME:-/root/.heiwa}"
HEIWA_NET_POLICY_TARGET="$HEIWA_HOME_DIR/policy/internet/net_policy_v2.json"
HEIWA_NET_POLICY_BOOTSTRAP_PATH="${HEIWA_NET_POLICY_BOOTSTRAP_PATH:-/app/policies/net_policy_v2.cloud_hq.json}"
if [[ ! -f "$HEIWA_NET_POLICY_TARGET" && -f "$HEIWA_NET_POLICY_BOOTSTRAP_PATH" ]]; then
    mkdir -p "$(dirname "$HEIWA_NET_POLICY_TARGET")"
    cp "$HEIWA_NET_POLICY_BOOTSTRAP_PATH" "$HEIWA_NET_POLICY_TARGET"
    echo "[HEIWA] Bootstrapped net policy."
fi

# 3. Inject CLI tool credentials from environment variables.
# These values are provided by the operator/runtime shell and are translated
# into the files/env vars that each CLI tool expects on headless Linux.

# Claude Code: uses env vars directly
if [[ -n "${CLAUDE_CODE_OAUTH_TOKEN:-}" ]]; then
    echo "[HEIWA] Claude Code OAuth token already set."
elif [[ -n "${CLAUDE_OAUTH_ACCESS_TOKEN:-}" ]]; then
    export CLAUDE_CODE_OAUTH_TOKEN="$CLAUDE_OAUTH_ACCESS_TOKEN"
    echo "[HEIWA] Claude Code OAuth token injected."
fi
if [[ -z "${CLAUDE_CODE_OAUTH_REFRESH_TOKEN:-}" && -n "${CLAUDE_OAUTH_REFRESH_TOKEN:-}" ]]; then
    export CLAUDE_CODE_OAUTH_REFRESH_TOKEN="$CLAUDE_OAUTH_REFRESH_TOKEN"
    export CLAUDE_CODE_OAUTH_SCOPES="openid profile email offline_access"
fi
export CLAUDE_CODE_ENVIRONMENT_KIND="${CLAUDE_CODE_ENVIRONMENT_KIND:-cloud}"

# Codex: writes ~/.codex/auth.json
if [[ -n "${CODEX_OAUTH_REFRESH_TOKEN:-}" ]]; then
    mkdir -p /root/.codex
    CODEX_ACCOUNT_ID="${CODEX_ACCOUNT_ID:-}"
    cat > /root/.codex/auth.json <<CODEX_AUTH
{
  "auth_mode": "chatgpt",
  "OPENAI_API_KEY": "",
  "tokens": {
    "id_token": "",
    "access_token": "",
    "refresh_token": "${CODEX_OAUTH_REFRESH_TOKEN}",
    "account_id": "${CODEX_ACCOUNT_ID}"
  },
  "last_refresh": "1970-01-01T00:00:00.000Z"
}
CODEX_AUTH
    echo "[HEIWA] Codex auth.json written."
fi

# Gemini CLI: writes ~/.gemini/oauth_creds.json
if [[ -n "${GEMINI_OAUTH_REFRESH_TOKEN:-}" ]]; then
    mkdir -p /root/.gemini
    cat > /root/.gemini/oauth_creds.json <<GEMINI_AUTH
{
  "access_token": "expired",
  "refresh_token": "${GEMINI_OAUTH_REFRESH_TOKEN}",
  "scope": "https://www.googleapis.com/auth/userinfo.profile openid https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email",
  "token_type": "Bearer",
  "id_token": "expired",
  "expiry_date": 0
}
GEMINI_AUTH
    echo "[HEIWA] Gemini oauth_creds.json written."
fi

# GitHub CLI: uses GH_TOKEN/GITHUB_TOKEN env vars directly.
GH_EFFECTIVE_TOKEN="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
if [[ -n "$GH_EFFECTIVE_TOKEN" ]]; then
    export GH_TOKEN="$GH_EFFECTIVE_TOKEN"
    export GITHUB_TOKEN="${GITHUB_TOKEN:-$GH_EFFECTIVE_TOKEN}"
    export GH_PROMPT_DISABLED=1
    echo "[HEIWA] GitHub CLI token injected."
fi

# Commit Signing: Setup GPG if HEIWA_GPG_KEY is provided
if [[ -n "${HEIWA_GPG_KEY:-}" ]]; then
    echo "[HEIWA] Importing GPG key for commit signing..."
    echo "$HEIWA_GPG_KEY" | base64 -d | gpg --import --batch --no-tty
    GPG_KEY_ID=$(gpg --list-secret-keys --keyid-format LONG | grep sec | awk '{print $2}' | cut -d'/' -f2 | head -1)
    if [[ -n "$GPG_KEY_ID" ]]; then
        git config --global user.signingkey "$GPG_KEY_ID"
        git config --global commit.gpgsign true
        echo "[HEIWA] GPG signing configured for key $GPG_KEY_ID."
    fi
fi

# Wrangler: uses CLOUDFLARE_API_TOKEN directly.
if [[ -n "${CLOUDFLARE_API_TOKEN:-}" ]]; then
    export CLOUDFLARE_API_TOKEN
    echo "[HEIWA] Cloudflare API token injected."
fi

# 4. Verify CLI tool availability and auth
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
if command -v gh &>/dev/null; then
    GH_VER=$(gh --version 2>/dev/null | head -1)
    echo "  gh:       $GH_VER"
    if [[ -n "${GH_TOKEN:-${GITHUB_TOKEN:-}}" ]]; then
        if gh auth status >/tmp/heiwa-gh-auth.log 2>&1; then
            echo "  gh:       auth configured (GH_TOKEN/GITHUB_TOKEN)"
        else
            echo "  gh:       WARNING — auth check failed (see /tmp/heiwa-gh-auth.log)"
        fi
    else
        echo "  gh:       WARNING — no auth token (set GH_TOKEN or GITHUB_TOKEN)"
    fi
else
    echo "  gh:       not installed"
fi
if command -v gpg &>/dev/null; then
    echo "  gpg:      available"
    if git config --get commit.gpgsign &>/dev/null; then
        echo "  signing:  configured (GPG)"
    else
        echo "  signing:  not configured"
    fi
fi

# 5. Launch the Rust Core
if [[ ! -f "/usr/local/bin/heiwa-core" ]]; then
    echo "[HEIWA] ERROR: /usr/local/bin/heiwa-core not found." >&2
    exit 1
fi

echo "[HEIWA] Launching heiwa-core..."
exec /usr/local/bin/heiwa-core
