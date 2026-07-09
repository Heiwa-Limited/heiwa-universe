#!/usr/bin/env bash
set -euo pipefail
export HOME="${HOME:-/home/devon}"
if [ -z "${HOME}" ] || echo "${HOME}" | grep -qE 'Users|^[A-Za-z]:'; then
  export HOME="$(getent passwd "$(id -un)" | cut -d: -f6)"
fi
cd "$HOME/heiwa/services/mcp-gateway"
# shellcheck disable=SC1091
source .venv/bin/activate
if [ -f "$HOME/heiwa/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  source "$HOME/heiwa/.env"
  set +a
fi
exec python -m heiwa_mcp_gateway --http --host "${HEIWA_MCP_HOST:-127.0.0.1}" --port "${HEIWA_MCP_PORT:-8742}" --token "${HEIWA_MCP_TOKEN:-}"
