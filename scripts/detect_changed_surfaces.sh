#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  detect_changed_surfaces.sh --all
  detect_changed_surfaces.sh <base-sha> <head-sha>
EOF
  exit 1
}

emit_all() {
  cat <<'EOF'
rust_terminal=true
typescript=true
python_reference=true
legacy_hub=true
docs=true
web_static=true
agent_sync=true
deploy_core=true
deploy_trading=true
deploy_web=true
EOF
}

if [[ $# -eq 1 && "$1" == "--all" ]]; then
  emit_all
  exit 0
fi

if [[ $# -ne 2 ]]; then
  usage
fi

base_sha="$1"
head_sha="$2"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"

rust_terminal=false
typescript=false
python_reference=false
legacy_hub=false
docs=false
web_static=false
agent_sync=false
deploy_core=false
deploy_trading=false
deploy_web=false

while IFS= read -r file; do
  [[ -z "$file" ]] && continue

  case "$file" in
    .github/workflows/*)
      rust_terminal=true
      typescript=true
      python_reference=true
      legacy_hub=true
      docs=true
      web_static=true
      agent_sync=true
      ;;
  esac

  case "$file" in
    Cargo.toml|Cargo.lock|rust-toolchain.toml|apps/heiwa_core/*|apps/heiwa_orchestrator/*|apps/heiwa_shell/*|crates/*|packages/heiwa_bindings/rust/*|scripts/check_heiwa_core_dockerfile.sh|scripts/check_runtime_baseline.sh)
      rust_terminal=true
      ;;
  esac

  case "$file" in
    apps/heiwa_core/*|apps/heiwa_orchestrator/*|crates/heiwa_stdb/*|railway.toml|infra/*|config/*)
      deploy_core=true
      ;;
  esac

  case "$file" in
    package.json|package-lock.json|tsconfig.base.json|.nvmrc|apps/heiwa_app/*|packages/heiwa_bindings/typescript/*)
      typescript=true
      web_static=true
      deploy_web=true
      ;;
  esac

  case "$file" in
    mkdocs.yml|docs/*|site/*)
      docs=true
      ;;
  esac

  case "$file" in
    scripts/sync_agents.py|AGENTS.md|CLAUDE.md|GEMINI.md|CONTRIBUTORS.md|.claude/*|.codex/*|.gemini/*|ops/agents/*)
      agent_sync=true
      ;;
  esac

  case "$file" in
    requirements.txt|pyproject.toml|uv.lock|conftest.py|packages/heiwa_cli/*|packages/heiwa_identity/*|packages/heiwa_protocol/*|packages/heiwa_sdk/*|legacy/packages/heiwa_cognition/*|legacy/packages/heiwa_ui/*|scripts/*|tests/*)
      python_reference=true
      legacy_hub=true
      deploy_trading=true
      ;;
  esac

  case "$file" in
    apps/heiwa_trading/*)
      python_reference=true
      deploy_trading=true
      ;;
  esac

  case "$file" in
    legacy/apps/heiwa_cli/*|legacy/apps/heiwa_hub/*)
      python_reference=true
      legacy_hub=true
      ;;
  esac
done < <(git -C "$repo_root" diff --name-only "$base_sha" "$head_sha" || true)

cat <<EOF
rust_terminal=$rust_terminal
typescript=$typescript
python_reference=$python_reference
legacy_hub=$legacy_hub
docs=$docs
web_static=$web_static
agent_sync=$agent_sync
deploy_core=$deploy_core
deploy_trading=$deploy_trading
deploy_web=$deploy_web
EOF
