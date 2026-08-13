#!/usr/bin/env bash
set -euo pipefail

shell_packages=(
  heiwa-shell
)

runtime_packages=(
  heiwa-core
  heiwa-loop
  heiwa-orchestrator
  heiwa-provider
  heiwa-session
  heiwa_drex
  heiwa_vault
)

foundation_packages=(
  heiwa-a2a
  heiwa-install
  heiwa-memory
  heiwa-protocol
  heiwa-repl
  heiwa-resource
  heiwa-tui
  heiwa_automations
  heiwa_config
  heiwa_embed
  heiwa_evidence
  heiwa_mcp
  heiwa_quota
  heiwa_receipts
)

validate_groups() {
  local expected actual drift
  expected="$({
    printf '%s\n' "${shell_packages[@]}"
    printf '%s\n' "${runtime_packages[@]}"
    printf '%s\n' "${foundation_packages[@]}"
  } | LC_ALL=C sort)"
  actual="$(
    cargo metadata --locked --no-deps --format-version 1 |
      jq -r '.packages[].name' |
      grep -v '^heiwa-desktop$' |
      LC_ALL=C sort
  )"
  drift="$(comm -3 <(printf '%s\n' "$expected") <(printf '%s\n' "$actual"))"
  if [[ -n "$drift" ]]; then
    printf '%s\n' 'Rust CI test groups do not match the non-desktop workspace:' >&2
    printf '%s\n' "$drift" >&2
    return 1
  fi
}

validate_groups

group="${1:-}"
if [[ "$group" == "--check" ]]; then
  printf '%s\n' 'Rust CI test groups cover every non-desktop workspace package exactly once.'
  exit 0
fi

case "$group" in
  shell) packages=("${shell_packages[@]}") ;;
  runtime) packages=("${runtime_packages[@]}") ;;
  foundation) packages=("${foundation_packages[@]}") ;;
  *)
    printf 'usage: %s {shell|runtime|foundation|--check}\n' "$0" >&2
    exit 2
    ;;
esac

package_args=()
for package in "${packages[@]}"; do
  package_args+=(--package "$package")
done

exec cargo nextest run \
  --locked \
  --no-default-features \
  "${package_args[@]}"
