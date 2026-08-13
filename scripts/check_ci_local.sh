#!/usr/bin/env bash
# Run exactly what .github/workflows/ci.yml runs, locally, before pushing.
#
# Why this exists: the rust-matrix job failed four times in a row on this
# branch, each time for a different reason, because "it passes locally" was
# being said about WEAKER commands than CI actually runs:
#
#   - bare `cargo clippy` reports warnings; CI uses `-D warnings` with a
#     specific allow-list, so every warning is a hard error there
#   - `cargo test` without `--locked` tolerates a stale Cargo.lock
#   - `cargo machete` was never run locally at all
#   - the workspace builds fine on a Mac with Homebrew protoc; no runner has it
#
# Each round-trip to CI costs ~15 minutes. This script costs a few minutes and
# catches the same things. Keep it in sync with ci.yml.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

FAILED=()
step() {
  local name="$1"; shift
  printf '  %-34s ' "$name"
  if "$@" >/tmp/heiwa_ci_step.log 2>&1; then
    echo "OK"
  else
    echo "FAIL"
    FAILED+=("$name")
    sed -n '1,25p' /tmp/heiwa_ci_step.log | sed 's/^/      /'
  fi
}

echo "== toolchain prerequisites =="
printf '  %-34s ' "protoc (lance default feature)"
if command -v protoc >/dev/null 2>&1; then echo "OK ($(protoc --version))"
else echo "MISSING - lance-encoding will not build"; FAILED+=("protoc"); fi

echo
echo "== rust matrix (ci.yml: rust-matrix) =="
step "cargo fmt --check" cargo fmt --all -- --check
step "cargo test --locked" cargo test --workspace --exclude heiwa-desktop --locked
step "cargo clippy -D warnings" cargo clippy --workspace --exclude heiwa-desktop --locked --all-targets -- \
  -A clippy::too_many_arguments -A clippy::new_without_default -A clippy::unnecessary_to_owned \
  -A clippy::needless_range_loop -A clippy::approx_constant -A clippy::collapsible_if \
  -A clippy::bool_assert_comparison -A clippy::type_complexity \
  -A clippy::needless_borrows_for_generic_args -A clippy::unnecessary_unwrap -D warnings
step "cargo machete" cargo machete

echo
echo "== web + docs (ci.yml: lint, docs) =="
step "npm run typecheck" npm run typecheck
step "npm run lint" npm run lint

echo
echo "== python =="
step "pytest" python3 -m pytest -q
step "just test-product" just test-product

echo
echo "== repo gates =="
for s in check_agent_baseline check_backend_transition check_model_call_boundary \
         check_release_metadata check_runtime_baseline verify_security check_machine_security \
         check_heiwa_core_dockerfile check_workflow_pins check_public_installer; do
  step "$s" bash "scripts/$s.sh"
done

echo
if [ ${#FAILED[@]} -eq 0 ]; then
  echo "ALL GREEN — safe to push."
else
  echo "FAILED (${#FAILED[@]}): ${FAILED[*]}"
  exit 1
fi
