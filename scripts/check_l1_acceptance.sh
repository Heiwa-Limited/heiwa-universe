#!/usr/bin/env bash
set -euo pipefail

# L1 acceptance gate — roadmap 2026-08-14, layer L1 (BYOK provider tier).
#
# Deterministic checks:
#   1. heiwa-provider unit tests pass (direct-API adapters included)
#   2. direct-API adapter modules exist for the three CLI-dependent families
#   3. fresh-install harness passes: no provider CLI on PATH + one API key
#      (mock provider server) completes an operator turn end to end
#   4. zero-provider state is usable: turn submission with no accounts yields
#      an actionable blocker, not a crash (asserted inside the harness)
#
# Local-only; the mock provider server binds loopback. No external network.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail=0
ok() { printf 'OK: %s\n' "$*"; }
fail_msg() { printf 'FAIL: %s\n' "$*" >&2; fail=1; }

# ── 2. Adapter modules exist ────────────────────────────────────────────────
for adapter in anthropic_api openai_api gemini_api; do
  if [[ -f "crates/heiwa_provider/src/providers/${adapter}.rs" ]]; then
    ok "direct-API adapter present: ${adapter}"
  else
    fail_msg "direct-API adapter missing: crates/heiwa_provider/src/providers/${adapter}.rs"
  fi
done

# ── 1. Provider crate tests ─────────────────────────────────────────────────
if cargo test -p heiwa-provider --quiet >/tmp/l1_provider_tests.log 2>&1; then
  ok "heiwa-provider tests"
else
  fail_msg "heiwa-provider tests (see /tmp/l1_provider_tests.log)"
fi

# ── 3+4. Fresh-install harness ──────────────────────────────────────────────
# The harness itself scrubs PATH of provider CLIs, uses a temp HEIWA_HOME,
# registers one API-key account against a loopback mock provider, and drives
# a full operator turn. It also asserts the zero-account blocker path.
if [[ -f "apps/heiwa_shell/tests/fresh_install.rs" ]]; then
  ok "fresh-install harness present"
  if cargo test -p heiwa-shell --test fresh_install --quiet >/tmp/l1_fresh_install.log 2>&1; then
    ok "fresh-install harness passes (no CLI on PATH + one API key → full turn)"
  else
    fail_msg "fresh-install harness failed (see /tmp/l1_fresh_install.log)"
  fi
else
  fail_msg "fresh-install harness missing: apps/heiwa_shell/tests/fresh_install.rs"
fi

if (( fail != 0 )); then
  printf 'L1 acceptance gate FAILED.\n' >&2
  exit 1
fi
mkdir -p .claude && git rev-parse HEAD > .claude/l1-accept-sha
printf 'L1 acceptance gate passed (stamp written for HEAD).\n'
