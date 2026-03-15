#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
MODULE_PATH="$ROOT/apps/heiwa_hub/spacetimedb"
OUT_ROOT="$ROOT/packages/heiwa_bindings"

mkdir -p "$OUT_ROOT/typescript" "$OUT_ROOT/rust"

spacetime generate --lang typescript --module-path "$MODULE_PATH" --out-dir "$OUT_ROOT/typescript" --yes
spacetime generate --lang rust --module-path "$MODULE_PATH" --out-dir "$OUT_ROOT/rust" --yes

echo "Generated SpacetimeDB TypeScript bindings at $OUT_ROOT/typescript"
echo "Generated SpacetimeDB Rust bindings at $OUT_ROOT/rust"
