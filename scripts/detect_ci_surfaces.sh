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
lance=false
dependency_security=false
desktop=false

classify_path() {
  local file="$1"

  case "$file" in
    .github/workflows/ci.yml|scripts/detect_ci_surfaces.sh|scripts/tests/test_detect_ci_surfaces.sh)
      rust=true
      lance=true
      dependency_security=true
      desktop=true
      return
      ;;
  esac

  case "$file" in
    Cargo.toml|Cargo.lock|rust-toolchain.toml|.cargo/*)
      rust=true
      lance=true
      dependency_security=true
      desktop=true
      ;;
    apps/*.rs|apps/*/Cargo.toml|crates/*.rs|crates/*/Cargo.toml|packages/heiwa_bindings/rust/*)
      rust=true
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
lance=$lance
dependency_security=$dependency_security
desktop=$desktop
EOF
}

if [[ $# -eq 1 && "$1" == "--all" ]]; then
  rust=true
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
