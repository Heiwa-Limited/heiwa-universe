#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
detector="$repo_root/scripts/detect_ci_surfaces.sh"
workflow="$repo_root/.github/workflows/ci.yml"

assert_case() {
  local label="$1"
  local expected="$2"
  shift 2
  local actual

  actual="$(printf '%s\n' "$@" | bash "$detector" --paths)"
  if [[ "$actual" != "$expected" ]]; then
    printf 'FAIL: %s\nexpected:\n%s\nactual:\n%s\n' "$label" "$expected" "$actual" >&2
    exit 1
  fi
}

none=$'rust=false\nlance=false\ndependency_security=false\ndesktop=false'
rust_only=$'rust=true\nlance=false\ndependency_security=false\ndesktop=false'
lance=$'rust=true\nlance=true\ndependency_security=false\ndesktop=false'
dependencies=$'rust=false\nlance=false\ndependency_security=true\ndesktop=false'
all=$'rust=true\nlance=true\ndependency_security=true\ndesktop=true'

assert_case "release workflow does not compile Rust" "$none" ".github/workflows/release.yml"
assert_case "docs do not compile Rust" "$none" "docs/backlog.md"
assert_case "ordinary Rust source uses the fast Rust lane" "$rust_only" "crates/heiwa_config/src/lib.rs"
assert_case "Lance source enables targeted Lance certification" "$lance" "crates/heiwa_embed/src/lance_store.rs"
assert_case "Node lockfile enables dependency security only" "$dependencies" "package-lock.json"
assert_case "Gitleaks policy uses the mandatory scan without dependency audits" "$none" ".gitleaksignore"
assert_case "CI workflow changes exercise every CI surface" "$all" ".github/workflows/ci.yml"
assert_case "root Rust metadata exercises every Rust target" "$all" "Cargo.lock"

if ! grep -Fq 'git show "$BASE_SHA:scripts/detect_ci_surfaces.sh"' "$workflow"; then
  echo "FAIL: pull-request classification must execute protected base policy" >&2
  exit 1
fi

printf 'CI surface detector tests passed.\n'
