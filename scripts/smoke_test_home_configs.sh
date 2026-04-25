#!/bin/bash
# Smoke test for Heiwa provider configurations and hooks

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

echo "--- Heiwa Home Config Smoke Test ---"

# 1. Validate JSON for Claude
echo -n "Checking Claude config (JSON)... "
if python3 -m json.tool ~/.claude/settings.json > /dev/null; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAILED${NC}"
    exit 1
fi

# 2. Validate JSON for Gemini
echo -n "Checking Gemini config (JSON)... "
if python3 -m json.tool ~/.gemini/settings.json > /dev/null; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAILED${NC}"
    exit 1
fi

# 3. Validate TOML for Codex (minimal check using python)
echo -n "Checking Codex config (TOML)... "
if python3 -c "import tomllib; tomllib.load(open('/Users/dmcgregsauce/.codex/config.toml', 'rb'))" > /dev/null 2>&1 || \
   python3 -c "import tomli; tomli.load(open('/Users/dmcgregsauce/.codex/config.toml', 'rb'))" > /dev/null 2>&1 || \
   grep -q "model" ~/.codex/config.toml; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAILED${NC}"
    exit 1
fi

# 4. Verify Hook Existence
echo "Checking Hook paths..."

HOOKS=(
    "/Users/dmcgregsauce/.claude/hooks/runtime_safety.ts"
    "/Users/dmcgregsauce/.claude/hooks/session_context.ts"
    "/Users/dmcgregsauce/.gemini/hooks/dangerous_check.ts"
    "/Users/dmcgregsauce/.gemini/hooks/inject_date.ts"
    "/Users/dmcgregsauce/.gemini/hooks/heiwa_bootstrap.ts"
)

for hook in "${HOOKS[@]}"; do
    echo -n "  $hook... "
    if [ -f "$hook" ]; then
        echo -e "${GREEN}EXISTS${NC}"
    else
        echo -e "${RED}MISSING${NC}"
        exit 1
    fi
done

echo -e "\n${GREEN}ALL CHECKS PASSED${NC}"
