#!/bin/bash
# scripts/init_env.sh
# Standardized environment anchor for Heiwa agent sessions.

echo "[HEIWA] Initializing Harness Environment..."

# 1. Working Directory Check
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"
echo "  - Root: $PROJECT_ROOT"

# 2. PYTHONPATH Alignment (Progressive Disclosure Map)
export PYTHONPATH="$PROJECT_ROOT/packages/heiwa_cli:$PROJECT_ROOT/packages/heiwa_cognition:$PROJECT_ROOT/packages/heiwa_sdk:$PROJECT_ROOT/packages/heiwa_protocol:$PROJECT_ROOT/packages/heiwa_identity:$PROJECT_ROOT/packages/heiwa_ui:$PROJECT_ROOT/apps"
echo "  - PYTHONPATH established."

# 3. Virtual Environment Check
if [ -d ".venv" ]; then
    source .venv/bin/activate
    echo "  - Virtualenv active."
else
    echo "  - WARNING: .venv not found. Run setup first."
fi

# 4. SpacetimeDB Connectivity Check
if command -v spacetime &> /dev/null; then
    echo "  - STDB CLI available."
else
    echo "  - WARNING: SpacetimeDB CLI not found in PATH."
fi

# 5. Git & GitHub Initialization
if [[ -n "${GH_TOKEN:-${GITHUB_TOKEN:-}}" ]]; then
    export GH_TOKEN="${GH_TOKEN:-$GITHUB_TOKEN}"
    export GITHUB_TOKEN="$GH_TOKEN"
    export GH_PROMPT_DISABLED=1
    echo "  - GitHub CLI auth established."
fi

if [[ -n "${HEIWA_GPG_KEY:-}" ]]; then
    echo "  - Importing GPG key..."
    echo "$HEIWA_GPG_KEY" | base64 -d | gpg --import --batch --no-tty &>/dev/null
    GPG_KEY_ID=$(gpg --list-secret-keys --keyid-format LONG | grep sec | awk '{print $2}' | cut -d'/' -f2 | head -1)
    if [[ -n "$GPG_KEY_ID" ]]; then
        git config user.signingkey "$GPG_KEY_ID"
        git config commit.gpgsign true
        echo "  - GPG signing active ($GPG_KEY_ID)."
    fi
fi

# 6. Git Status
echo "  - Recent work:"
git log --oneline -5

echo "[HEIWA] Environment Ready."
