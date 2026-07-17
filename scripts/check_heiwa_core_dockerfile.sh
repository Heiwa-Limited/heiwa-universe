#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

dockerfile="apps/heiwa_core/Dockerfile"
entrypoint="apps/heiwa_core/start.sh"
minimum_major=1
minimum_minor=93

builder_line="$(grep -E '^FROM rust:[0-9]+\.[0-9]+-slim AS rust-builder$' "$dockerfile" || true)"
if [[ -z "$builder_line" ]]; then
  echo "Could not find rust-builder image pin in $dockerfile" >&2
  exit 1
fi

version="${builder_line#FROM rust:}"
version="${version%-slim AS rust-builder}"
major="${version%%.*}"
minor="${version#*.}"

if (( major < minimum_major || (major == minimum_major && minor < minimum_minor) )); then
  echo "$dockerfile pins rust-builder to $major.$minor, but heiwa-core requires at least $minimum_major.$minimum_minor" >&2
  exit 1
fi

while IFS= read -r source; do
  if [[ ! -e "$source" ]]; then
    echo "$dockerfile copies missing build-context path: $source" >&2
    exit 1
  fi
done < <(awk '$1 == "COPY" && $2 !~ /^--from=/ { for (i = 2; i < NF; i++) print $i }' "$dockerfile")

if grep -Eqi 'spacetime|STDB_|heiwa_bindings' "$dockerfile" "$entrypoint"; then
  echo "heiwa-core container still references the retired SpacetimeDB backend" >&2
  exit 1
fi

echo "$dockerfile pins rust-builder to $major.$minor, meeting the minimum $minimum_major.$minimum_minor requirement."
