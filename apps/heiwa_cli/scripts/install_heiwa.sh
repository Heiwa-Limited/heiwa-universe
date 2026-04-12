#!/usr/bin/env bash
set -euo pipefail

ROOT="${HEIWA_ROOT:-$HOME/.heiwa}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo required for source install in current OSS phase" >&2
  exit 1
fi

mkdir -p "$ROOT/bin"
cargo install --path apps/heiwa_shell --root "$ROOT" --force
"$ROOT/bin/heiwa" install
