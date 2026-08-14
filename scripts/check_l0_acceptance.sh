#!/usr/bin/env bash
set -euo pipefail

# L0 acceptance gate — roadmap 2026-08-14, layer L0 (UI foundation + N-user config root).
#
# Deterministic checks:
#   1. desktop typecheck + production build succeed
#   2. desktop vitest suite passes (operator seam + surface render tests)
#   3. operator seam test files are byte-identical to the pre-migration baseline
#   4. all ten surfaces exist as component modules and a shell render test exists
#   5. no home-path resolution outside the ConfigRoot resolver (runtime code)
#   6. design tokens: no raw hex colors outside the theme layer
#
# This script is local-only and does not use the network.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

desktop="apps/heiwa_app/desktop"
fail=0

ok() { printf 'OK: %s\n' "$*"; }
fail_msg() { printf 'FAIL: %s\n' "$*" >&2; fail=1; }

# ── 1+2. Desktop typecheck, build, tests ────────────────────────────────────
if (cd "$desktop" && npm run --silent typecheck >/tmp/l0_typecheck.log 2>&1); then
  ok "desktop typecheck"
else
  fail_msg "desktop typecheck (see /tmp/l0_typecheck.log)"
fi

if (cd "$desktop" && npm run --silent build >/tmp/l0_build.log 2>&1); then
  ok "desktop production build"
else
  fail_msg "desktop production build (see /tmp/l0_build.log)"
fi

if (cd "$desktop" && npm test --silent >/tmp/l0_vitest.log 2>&1); then
  ok "desktop vitest suite"
else
  fail_msg "desktop vitest suite (see /tmp/l0_vitest.log)"
fi

# ── 3. Operator seam preserved: test files byte-identical to baseline ───────
declare -A seam_baseline=(
  ["$desktop/src/operator/store.test.ts"]="7f68b72bc113940349648ef505bc49b52ecd11d21410b046b05fee06b8e6b2a0"
  ["$desktop/src/operator/client.test.ts"]="a162fe8e094baf8f497504c9e99761ad069b8e5c614321efea4a34ab0ebb8470"
)
for file in "${!seam_baseline[@]}"; do
  if [[ ! -f "$file" ]]; then
    fail_msg "seam test missing: $file"
    continue
  fi
  actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  if [[ "$actual" == "${seam_baseline[$file]}" ]]; then
    ok "seam test unmodified: ${file#"$desktop"/}"
  else
    fail_msg "seam test modified since baseline: $file (operator seam must be preserved; if a seam change was deliberately approved, update the baseline hash in this script in the same commit)"
  fi
done

# ── 4. Ten surfaces as component modules ────────────────────────────────────
surfaces=(home ai windows calendar mail finance social workers browser files)
missing_surfaces=0
for surface in "${surfaces[@]}"; do
  if ! ls "$desktop/src/surfaces/$surface"/*.tsx >/dev/null 2>&1; then
    fail_msg "surface module missing: src/surfaces/$surface/"
    missing_surfaces=1
  fi
done
[[ $missing_surfaces -eq 0 ]] && ok "all ten surface modules present"

if ls "$desktop/src/shell"/*.test.tsx >/dev/null 2>&1 || ls "$desktop/src/surfaces"/*.test.tsx >/dev/null 2>&1 || ls "$desktop/src"/app.test.tsx >/dev/null 2>&1; then
  ok "shell/surface render test present"
else
  fail_msg "no shell/surface render test found (need a vitest that mounts the shell and renders every surface)"
fi

# ── 5. No home-path resolution outside ConfigRoot (runtime code) ────────────
# The invariant is about path *construction*, not about the string "~/.heiwa"
# appearing in help text: only crates/heiwa_config may read HOME/USERPROFILE,
# call dirs::home_dir(), or join a ".heiwa" root. Prose in println!/json! that
# documents the default location is not a violation.
#
# Test modules are skipped from the first `#[cfg(test)]` line to EOF (Rust
# convention places them last), since hermetic tests legitimately build their
# own temp roots.
strip_tests() {
  awk '/^#\[cfg\(test\)\]/ { exit } { print FILENAME ":" FNR ":" $0 }' "$1"
}

resolver_violations=""
while IFS= read -r file; do
  [[ "$file" == crates/heiwa_config/src/lib.rs ]] && continue
  hits="$(strip_tests "$file" \
    | grep -E 'env::var(_os)?\("(HOME|USERPROFILE)"\)|dirs::home_dir|join\("\.heiwa"\)' \
    || true)"
  [[ -n "$hits" ]] && resolver_violations+="$hits"$'\n'
done < <(find apps/heiwa_core/src apps/heiwa_shell/src apps/heiwa_orchestrator/src crates \
  -name '*.rs' -not -path '*/tests/*' -not -name '*_test.rs' 2>/dev/null | sort)

resolver_violations="$(printf '%s' "$resolver_violations" | grep -v '^$' || true)"
if [[ -z "$resolver_violations" ]]; then
  ok "no independent home/state-root resolution outside ConfigRoot"
else
  count="$(printf '%s\n' "$resolver_violations" | wc -l | tr -d ' ')"
  fail_msg "$count independent home/state-root resolution(s) outside ConfigRoot:"
  printf '%s\n' "$resolver_violations" | head -40 >&2
fi

identity_violations="$(grep -rn --include='*.rs' --include='*.ts' --include='*.tsx' \
  -e 'devon-canonical' -e 'dmcgregsauce' -e 'devon@heiwa' \
  apps/heiwa_core/src apps/heiwa_shell/src apps/heiwa_orchestrator/src crates "$desktop/src" \
  2>/dev/null | grep -v -e 'tests/' -e '_test\.' -e '\.test\.' || true)"
if [[ -z "$identity_violations" ]]; then
  ok "no hardcoded operator identity in runtime code"
else
  fail_msg "hardcoded operator identity present:"
  printf '%s\n' "$identity_violations" | head -20 >&2
fi

# ── 6. Token discipline: no raw hex colors outside theme layer ──────────────
if [[ -d "$desktop/src/theme" ]]; then
  hex_violations="$(grep -rn --include='*.css' -E '#[0-9a-fA-F]{3,8}\b' "$desktop/src" \
    | grep -v "^$desktop/src/theme/" || true)"
  if [[ -z "$hex_violations" ]]; then
    ok "styles consume tokens only (no raw hex outside theme/)"
  else
    fail_msg "raw color literals outside theme layer:"
    printf '%s\n' "$hex_violations" | head -20 >&2
  fi
else
  fail_msg "theme layer missing: $desktop/src/theme/"
fi

if (( fail != 0 )); then
  printf 'L0 acceptance gate FAILED.\n' >&2
  exit 1
fi
mkdir -p .claude && git rev-parse HEAD > .claude/l0-accept-sha
printf 'L0 acceptance gate passed (stamp written for HEAD).\n'
