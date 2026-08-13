#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  detect_ci_surfaces.sh --all
  detect_ci_surfaces.sh --paths       # read changed paths from stdin
  detect_ci_surfaces.sh <base-sha> <head-sha>
EOF
  exit 2
}

rust=false
rust_shell=false
rust_runtime=false
rust_foundation=false
lance=false
dependency_security=false
desktop=false

classify_path() {
  local file="$1"

  case "$file" in
    scripts/detect_ci_surfaces.sh|scripts/tests/test_detect_ci_surfaces.sh)
      rust=true
      rust_shell=true
      rust_runtime=true
      rust_foundation=true
      lance=true
      dependency_security=true
      desktop=true
      return
      ;;
  esac

  case "$file" in
    .github/workflows/ci.yml|.github/workflows/certification.yml|scripts/ci_rust_test_group.sh)
      return
      ;;
    apps/heiwa_app/desktop/*)
      desktop=true
      case "$file" in
        */Cargo.toml|*/package.json|*/package-lock.json) dependency_security=true ;;
      esac
      return
      ;;
  esac

  case "$file" in
    Cargo.toml|Cargo.lock|rust-toolchain.toml|.cargo/*)
      rust=true
      rust_shell=true
      rust_runtime=true
      rust_foundation=true
      lance=true
      dependency_security=true
      desktop=true
      ;;
    apps/heiwa_shell/*)
      rust=true
      rust_shell=true
      ;;
    apps/heiwa_core/*|crates/heiwa_loop/*|crates/heiwa_orchestrator/*|crates/heiwa_provider/*|crates/heiwa_session/*|crates/heiwa_drex/*|crates/heiwa_vault/*)
      rust=true
      rust_runtime=true
      ;;
    crates/heiwa_a2a/*|crates/heiwa_install/*|crates/heiwa_memory/*|crates/heiwa_protocol/*|crates/heiwa_repl/*|crates/heiwa_resource/*|crates/heiwa_tui/*|crates/heiwa_automations/*|crates/heiwa_config/*|crates/heiwa_embed/*|crates/heiwa_evidence/*|crates/heiwa_mcp/*|crates/heiwa_quota/*|crates/heiwa_receipts/*)
      rust=true
      rust_foundation=true
      ;;
    apps/*.rs|apps/*/Cargo.toml|crates/*.rs|crates/*/Cargo.toml|packages/heiwa_bindings/rust/*)
      rust=true
      rust_shell=true
      rust_runtime=true
      rust_foundation=true
      ;;
  esac

  case "$file" in
    apps/heiwa_shell/Cargo.toml|crates/heiwa_embed/*|crates/heiwa_session/*)
      lance=true
      ;;
  esac

  case "$file" in
    apps/heiwa_app/desktop/*)
      desktop=true
      ;;
  esac

  case "$file" in
    Cargo.toml|Cargo.lock|*/Cargo.toml|package.json|package-lock.json|*/package.json|*/package-lock.json|pyproject.toml|uv.lock|requirements.txt|*/pyproject.toml|*/uv.lock|*/requirements.txt|scripts/verify_security.sh)
      dependency_security=true
      ;;
  esac
}

emit() {
  cat <<EOF
rust=$rust
rust_shell=$rust_shell
rust_runtime=$rust_runtime
rust_foundation=$rust_foundation
lance=$lance
dependency_security=$dependency_security
desktop=$desktop
EOF
}

if [[ $# -eq 1 && "$1" == "--all" ]]; then
  rust=true
  rust_shell=true
  rust_runtime=true
  rust_foundation=true
  lance=true
  dependency_security=true
  desktop=true
  emit
  exit 0
fi

if [[ $# -eq 1 && "$1" == "--paths" ]]; then
  while IFS= read -r file; do
    [[ -n "$file" ]] && classify_path "$file"
  done
  emit
  exit 0
fi

if [[ $# -ne 2 ]]; then
  usage
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
while IFS= read -r file; do
  [[ -n "$file" ]] && classify_path "$file"
done < <(git -C "$repo_root" diff --name-only "$1" "$2")

emit
