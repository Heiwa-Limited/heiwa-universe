#!/usr/bin/env bash
set -euo pipefail

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

echo
echo "Recommended repo baseline"
echo "-------------------------"
echo "Rust toolchain : 1.95.0"
echo "Node runtime   : 26.0.0"
echo "Python         : 3.14.x"
echo "Required CLIs  : gh, uv"
echo "Optional CLIs  : wrangler, pnpm, ollama, tailscale"

if command -v brew >/dev/null 2>&1; then
  echo
  echo "Outdated Homebrew packages"
  echo "-------------------------"
  brew outdated --greedy-auto-updates || true
fi
