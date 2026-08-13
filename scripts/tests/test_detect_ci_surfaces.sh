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

none=$'rust=false\nrust_shell=false\nrust_runtime=false\nrust_foundation=false\nlance=false\ndependency_security=false\ndesktop=false'
shell=$'rust=true\nrust_shell=true\nrust_runtime=false\nrust_foundation=false\nlance=false\ndependency_security=false\ndesktop=false'
runtime=$'rust=true\nrust_shell=false\nrust_runtime=true\nrust_foundation=false\nlance=false\ndependency_security=false\ndesktop=false'
foundation=$'rust=true\nrust_shell=false\nrust_runtime=false\nrust_foundation=true\nlance=false\ndependency_security=false\ndesktop=false'
lance=$'rust=true\nrust_shell=false\nrust_runtime=false\nrust_foundation=true\nlance=true\ndependency_security=false\ndesktop=false'
dependencies=$'rust=false\nrust_shell=false\nrust_runtime=false\nrust_foundation=false\nlance=false\ndependency_security=true\ndesktop=false'
desktop=$'rust=false\nrust_shell=false\nrust_runtime=false\nrust_foundation=false\nlance=false\ndependency_security=false\ndesktop=true'
all=$'rust=true\nrust_shell=true\nrust_runtime=true\nrust_foundation=true\nlance=true\ndependency_security=true\ndesktop=true'

assert_case "release workflow does not compile Rust" "$none" ".github/workflows/release.yml"
assert_case "docs do not compile Rust" "$none" "docs/backlog.md"
assert_case "shell source selects only shell tests" "$shell" "apps/heiwa_shell/src/lib.rs"
assert_case "runtime source selects only runtime tests" "$runtime" "crates/heiwa_provider/src/lib.rs"
assert_case "foundation source selects only foundation tests" "$foundation" "crates/heiwa_config/src/lib.rs"
assert_case "desktop source stays in desktop certification" "$desktop" "apps/heiwa_app/desktop/src-tauri/src/main.rs"
assert_case "Lance source enables targeted Lance certification" "$lance" "crates/heiwa_embed/src/lance_store.rs"
assert_case "Node lockfile enables dependency security only" "$dependencies" "package-lock.json"
assert_case "Gitleaks policy uses the mandatory scan without dependency audits" "$none" ".gitleaksignore"
assert_case "CI workflow changes use policy checks, not product rebuilds" "$none" ".github/workflows/ci.yml"
assert_case "certification workflow changes use policy checks, not product rebuilds" "$none" ".github/workflows/certification.yml"
assert_case "Rust sharding changes use policy checks, not product rebuilds" "$none" "scripts/ci_rust_test_group.sh"
assert_case "root Rust metadata exercises every Rust target" "$all" "Cargo.lock"

if ! grep -Fq 'git show "$BASE_SHA:scripts/detect_ci_surfaces.sh"' "$workflow"; then
  echo "FAIL: pull-request classification must execute protected base policy" >&2
  exit 1
fi
if ! grep -Fq 'git -C "$GITHUB_WORKSPACE" diff --name-only "$BASE_SHA" "$HEAD_SHA"' "$workflow" ||
  ! grep -Fq 'bash "$trusted_classifier" --paths' "$workflow"; then
  echo "FAIL: trusted classifier must receive paths computed inside the checkout" >&2
  exit 1
fi

printf 'CI surface detector tests passed.\n'
