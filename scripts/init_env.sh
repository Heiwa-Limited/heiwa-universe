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

# 5. Git Status
echo "  - Recent work:"
git log --oneline -5

echo "[HEIWA] Environment Ready."
