#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

print_version() {
  local label="$1"
  local cmd="$2"

  if command -v "$cmd" >/dev/null 2>&1; then
    printf "%-18s %s\n" "$label" "$("$cmd" --version 2>/dev/null | head -n 1)"
  else
    printf "%-18s %s\n" "$label" "missing"
  fi
}

echo "Heiwa operator machine audit"
echo "==========================="
print_version "rustc" rustc
print_version "cargo" cargo
print_version "node" node
print_version "npm" npm
print_version "python3" python3
print_version "uv" uv
print_version "brew" brew
print_version "gh" gh
print_version "wrangler" wrangler
print_version "pnpm" pnpm
print_version "ollama" ollama
print_version "tailscale" tailscale

failures=0
required_node_version="$(tr -d '[:space:]' <"$root_dir/.node-version")"
required_node_major="${required_node_version%%.*}"
actual_node_version="$(node --version 2>/dev/null || true)"

if [[ "$actual_node_version" != "v${required_node_major}."* ]]; then
  echo "Node runtime mismatch: expected ${required_node_major}.x, found ${actual_node_version:-missing}" >&2
  failures=$((failures + 1))
fi

echo
echo "Recommended repo baseline"
echo "-------------------------"
echo "Rust toolchain : 1.95.0"
echo "Node runtime   : $required_node_version"
echo "Python         : 3.14.x"
echo "Required CLIs  : gh, uv"
echo "Optional CLIs  : wrangler, pnpm, ollama, tailscale"

if command -v brew >/dev/null 2>&1; then
  echo
  echo "Outdated Homebrew packages"
  echo "-------------------------"
  brew outdated --greedy-auto-updates || true
fi

exit "$failures"
