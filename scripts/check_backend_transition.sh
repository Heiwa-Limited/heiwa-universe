#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

current_surfaces=(
  README.md
  GEMINI.md
  .env.example
  apps/heiwa_app/clients/web/assets/domains.bootstrap.json
  apps/heiwa_app/clients/web/governance.html
  apps/heiwa_app/clients/web/index.html
  apps/heiwa_app/clients/web/domains.html
  apps/heiwa_app/scripts/check_static_surface.py
  apps/heiwa_core/Dockerfile
  apps/heiwa_core/start.sh
  docs/architecture.md
  docs/capability-fabric.md
  docs/current-capability.md
  docs/index.md
  docs/local-self-operation.md
  docs/product-contract.md
  docs/provider-registry.md
  docs/publishing.md
  docs/security.md
  docs/state-layout.md
  docs/standards/agent_standard_v1.md
  docs/standards/runtime-baseline.md
  ops/agents/heiwa-architect/agent.yaml
  ops/agents/heiwa-architect/prompt.md
  ops/agents/heiwa-architect/generated/codex/heiwa-architect/SKILL.md
  ops/rooms/control-plane.md
  ops/rooms/execution.md
  ops/rooms/infra.md
  scripts/audit_operator_machine.sh
  scripts/check_runtime_baseline.sh
  scripts/init_env.sh
)

if rg -n -i 'spacetimedb|\bSTDB(_|\b)|spacetime (login|publish|start)' "${current_surfaces[@]}"; then
  echo "Current runtime surfaces still reference the retired SpacetimeDB backend." >&2
  exit 1
fi

if ! cargo tree -p heiwa-shell -e features -i heiwa_embed | grep -q 'heiwa_embed feature "lance"'; then
  echo "heiwa-shell does not enable the Lance embedding backend in its production feature graph." >&2
  exit 1
fi

echo "Current runtime surfaces match the Lance + local JSONL backend contract."
