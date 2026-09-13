#!/usr/bin/env bash
# Regression: an acceptance gate stamps only the exact clean source its checks
# observed. Drives the real Work Fabric A1 gate in disposable Git fixtures with
# a deterministic `cargo` double, then guards that every acceptance gate uses
# the shared helper instead of stamping on its own.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
failures=0
fail() { printf 'FAIL: %s\n' "$*" >&2; failures=$((failures + 1)); }
pass() { printf 'ok: %s\n' "$*"; }

fixture_root=""
cleanup() { [[ -n "$fixture_root" ]] && rm -rf "$fixture_root"; }
trap cleanup EXIT

# new_fixture <mutation-at-last-check>
#   Prints the fixture directory. The cargo double records the HEAD each check
#   observed and runs <mutation> when the gate makes its final cargo call.
new_fixture() {
  local mutation="$1" dir
  dir="$(mktemp -d "$fixture_root/gate.XXXXXX")"
  mkdir -p "$dir/scripts/lib" "$dir/apps" "$dir/crates" "$dir/tools"
  cp "$repo_root/scripts/check_work_fabric_a1_acceptance.sh" "$dir/scripts/"
  cp "$repo_root/scripts/lib/verification_logs.sh" "$repo_root/scripts/lib/acceptance_stamp.sh" "$dir/scripts/lib/"
  printf '.claude/\nprivate/\ntools/\nchecks.txt\n' > "$dir/.gitignore"
  printf 'source A\n' > "$dir/apps/source.txt"
  local calls
  calls="$(grep -c '^  a1_.*\.log cargo test' "$repo_root/scripts/check_work_fabric_a1_acceptance.sh")"
  cat > "$dir/tools/cargo" <<CARGO
#!/usr/bin/env bash
set -euo pipefail
git rev-parse HEAD >> checks.txt
if [[ "\$(wc -l < checks.txt | tr -d ' ')" -eq $calls ]]; then
  $mutation
fi
CARGO
  chmod +x "$dir/tools/cargo"
  (
    cd "$dir"
    git init -q
    git config user.name "Heiwa fixture"
    git config user.email "fixture@example.invalid"
    git add .
    git commit -qm "source A"
  )
  printf '%s\n' "$dir"
}

run_gate() {
  local dir="$1"
  (cd "$dir" && env -u HEIWA_VERIFICATION_LOG_DIR PATH="$dir/tools:/usr/bin:/bin" \
    bash scripts/check_work_fabric_a1_acceptance.sh) >"$dir.out" 2>&1
}

stamp_of() { cat "$1/.claude/work-fabric-a1-accept-sha" 2>/dev/null || true; }

fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/heiwa-acceptance-stamp.XXXXXX")"

# 1. Clean and unchanged: the stamp names the revision every check observed.
dir="$(new_fixture ':')"
start="$(git -C "$dir" rev-parse HEAD)"
if run_gate "$dir" && [[ "$(stamp_of "$dir")" == "$start" ]] \
  && [[ "$(sort -u "$dir/checks.txt")" == "$start" ]]; then
  pass "clean unchanged source is stamped with the observed revision"
else
  fail "clean unchanged run did not stamp $start: $(cat "$dir.out")"
fi

# 2. A commit after the final check must not be stamped (review reproduction).
dir="$(new_fixture 'printf "source B\n" > apps/source.txt; git add apps/source.txt; git commit -qm "source B"')"
if run_gate "$dir"; then
  fail "gate passed although HEAD moved during checks: $(cat "$dir.out")"
elif [[ -n "$(stamp_of "$dir")" ]]; then
  fail "gate wrote a stamp for a revision no check observed"
elif ! grep -q "HEAD moved" "$dir.out"; then
  fail "moved HEAD was not named: $(cat "$dir.out")"
else
  pass "a revision committed during checks is rejected and never stamped"
fi

# 3. Untracked source appearing during checks is a source change.
dir="$(new_fixture 'printf "new\n" > apps/new_source.rs')"
if run_gate "$dir" || [[ -n "$(stamp_of "$dir")" ]]; then
  fail "untracked source added during checks was stamped or passed: $(cat "$dir.out")"
else
  pass "untracked source added during checks fails without a stamp"
fi

# 4. Dirty (untracked) at the start: becoming clean later is not stampable.
dir="$(new_fixture 'rm -f apps/wip.rs')"
printf 'wip\n' > "$dir/apps/wip.rs"
if run_gate "$dir" && [[ -z "$(stamp_of "$dir")" ]] \
  && grep -q "dirty when checks started" "$dir.out"; then
  pass "a run that starts dirty never stamps, even if it ends clean"
else
  fail "dirty-start run stamped or failed: $(cat "$dir.out")"
fi

# 5. Every acceptance gate stamps only through the shared helper.
for gate in "$repo_root"/scripts/check_*_acceptance.sh; do
  name="$(basename "$gate")"
  begin_line="$(grep -n '^acceptance_source_begin$' "$gate" | head -1 | cut -d: -f1 || true)"
  first_check="$(grep -nE '^(if |run_check |[a-z_]+=\$\(grep)' "$gate" | head -1 | cut -d: -f1 || true)"
  if [[ -z "$begin_line" ]] || ! grep -q '^acceptance_source_finish ' "$gate"; then
    fail "$name does not use scripts/lib/acceptance_stamp.sh"
  elif [[ -n "$first_check" && "$begin_line" -gt "$first_check" ]]; then
    fail "$name captures its source after checks begin (line $begin_line > $first_check)"
  elif grep -qE 'rev-parse HEAD *> *\.claude/' "$gate"; then
    fail "$name writes a stamp outside the shared helper"
  else
    pass "$name binds its stamp to the source observed at the start"
  fi
done

if (( failures > 0 )); then
  printf '%d acceptance stamp regression(s) failed.\n' "$failures" >&2
  exit 1
fi
printf 'acceptance stamp regressions passed.\n'
