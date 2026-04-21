#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
MODULE_PATH="$ROOT/apps/heiwa_hub/spacetimedb"
OUT_ROOT="$ROOT/packages/heiwa_bindings"
RUST_OUT="$OUT_ROOT/rust/generated"
TS_OUT="$OUT_ROOT/typescript/generated"

rm -rf "$RUST_OUT" "$TS_OUT"
mkdir -p "$RUST_OUT" "$TS_OUT"

spacetime generate --lang typescript --module-path "$MODULE_PATH" --out-dir "$TS_OUT" --yes
spacetime generate --lang rust --module-path "$MODULE_PATH" --out-dir "$RUST_OUT" --yes

echo "Generated SpacetimeDB TypeScript bindings at $TS_OUT"
echo "Generated SpacetimeDB Rust bindings at $RUST_OUT"
