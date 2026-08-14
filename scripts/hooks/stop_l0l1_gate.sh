#!/usr/bin/env bash
set -euo pipefail

# Claude Code Stop gate for the L0/L1 build (roadmap 2026-08-14).
#
# Blocks ending the session while the task ledger declares a layer complete
# (all rows `done`) but the corresponding acceptance gate has not passed at
# the current HEAD. The acceptance scripts write the stamp on success:
#   scripts/check_l0_acceptance.sh -> .claude/l0-accept-sha
#   scripts/check_l1_acceptance.sh -> .claude/l1-accept-sha
#
# A stop with work still in progress (ledger rows todo/doing/blocked) is
# always allowed — this gate only fires on unverified completion claims.
# Fast by design: grep + git rev-parse only, no builds.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

ledger="docs/superpowers/ledgers/2026-08-14-L0-L1-task-ledger.md"
[[ -f "$ledger" ]] || exit 0

head_sha="$(git rev-parse HEAD 2>/dev/null || echo none)"

layer_claims_complete() {
  local start="$1" end="$2"
  local section
  section="$(awk "/^## ${start}/,/^## ${end}/" "$ledger")"
  local done_rows pending_rows
  done_rows="$(printf '%s' "$section" | grep -c '| done |' || true)"
  pending_rows="$(printf '%s' "$section" | grep -Ec '\| (todo|doing|blocked[^|]*) \|' || true)"
  [[ "$done_rows" -gt 0 && "$pending_rows" -eq 0 ]]
}

stamp_fresh() {
  local stamp_file="$1"
  [[ -f "$stamp_file" ]] && [[ "$(cat "$stamp_file")" == "$head_sha" ]]
}

block() {
  # Stop-hook JSON: decision block + reason re-engages the model.
  printf '{"decision":"block","reason":"%s"}\n' "$1"
  exit 0
}

if layer_claims_complete "L0 " "L1 " && ! stamp_fresh ".claude/l0-accept-sha"; then
  block "Ledger declares L0 complete but scripts/check_l0_acceptance.sh has not passed at HEAD ($head_sha). Run it; on failure fix and rerun, or set the ledger rows back to their honest status."
fi

if layer_claims_complete "L1 " "Architectural" && ! stamp_fresh ".claude/l1-accept-sha"; then
  block "Ledger declares L1 complete but scripts/check_l1_acceptance.sh has not passed at HEAD ($head_sha). Run it; on failure fix and rerun, or set the ledger rows back to their honest status."
fi

exit 0
