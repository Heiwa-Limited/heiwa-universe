#!/usr/bin/env bash
set -euo pipefail

# acceptance-scope: apps crates Cargo.toml Cargo.lock scripts/check_work_fabric_a1_acceptance.sh scripts/lib/verification_logs.sh scripts/lib/acceptance_stamp.sh
#
# The claims below are behaviour of the Rust runtime and the shipped binary,
# so any change under apps/ or crates/ can invalidate them.

# Work Fabric A1 acceptance gate — ledger
# docs/superpowers/ledgers/2026-08-22-work-fabric-task-ledger.md, rows A1-c 8-10.
#
#   1. Home, Work, and Agent are selections of one Work-session snapshot: they
#      report one Work, revision, epoch, cursor, and bound, and refuse updates
#      from another Work, another fold, or a revision that is not next.
#   2. A run whose supervising process is gone is recorded stale once, inside
#      the exclusive restart-recovery section, with what was observed of its
#      process (alive, gone, unknown). Healthy owned work is never marked;
#      finished runs and earlier runs of the same worker keep their outcomes.
#   3. The built `heiwa` binary proves it end to end in an isolated runtime:
#      a completed run needs no recovery; killing a worker's owner leaves a
#      surviving child recorded alive and never stopped, relaunched, or
#      re-effected; a dead child is recorded gone; and an app runtime restart
#      records the loss before it serves.
#
# Local-only. Tests use temporary HEIWA_HOME roots and loopback; this gate
# never reads or writes durable operator evidence or the installed runtime.
# Not covered: desktop UI consumption of these surfaces, the raw-command
# Action Gate, and worktree sandbox enforcement.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
source "$repo_root/scripts/lib/verification_logs.sh"
source "$repo_root/scripts/lib/acceptance_stamp.sh"
acceptance_source_begin
umask 077
log_dir="$(verification_log_dir "$repo_root" "work-fabric-a1")"

fail=0
ok() { printf 'OK: %s\n' "$*"; }
fail_msg() { printf 'FAIL: %s\n' "$*" >&2; fail=1; }

run_check() {
  local label="$1" log="$2"
  shift 2
  if "$@" >"$log_dir/$log" 2>&1; then
    ok "$label"
  else
    fail_msg "$label (see $log_dir/$log)"
  fi
}

# ── 1. Surfaces and the run model ───────────────────────────────────────────
run_check "Work surfaces agree over one snapshot; runs fold per invocation" \
  a1_work_worker.log cargo test --locked -p heiwa_work -p heiwa_worker

# ── 2. Stale marker and exclusive recovery ──────────────────────────────────
run_check "worker_stale is a durable operator event type" \
  a1_event_type.log cargo test --locked -p heiwa_evidence --test operator_journal \
  worker_and_pane_event_types_round_trip_through_json
run_check "recovery planners run only inside the exclusive recovery section" \
  a1_session_recovery.log cargo test --locked -p heiwa-session --test operator_service restart_recovery
run_check "shell recovery observes processes and never marks owned work" \
  a1_shell_recovery.log cargo test --locked -p heiwa-shell --bin heiwa cmd::recover
run_check "the app API serves the same one-snapshot surfaces" \
  a1_surface_route.log cargo test --locked -p heiwa-shell --bin heiwa work_surfaces_route

# ── 3. The built binary, end to end ─────────────────────────────────────────
run_check "heiwa binary: surfaces, interrupted owner, dead child, app restart" \
  a1_binary.log cargo test --locked -p heiwa-shell --test work_fabric_a1 --test work_run

# ── 4. One place builds surfaces ────────────────────────────────────────────
# A second view builder is how two surfaces come to disagree.
second_builders=$(grep -rlE 'fn (home|work|agent)_view' apps/ crates/ --include='*.rs' 2>/dev/null \
  | grep -v '^crates/heiwa_work/src/surface.rs$' || true)
if [[ -n "$second_builders" ]]; then
  fail_msg "surface views are built outside heiwa_work::surface: $second_builders"
else
  ok "surface views have a single implementation"
fi

if (( fail != 0 )); then
  printf 'Work Fabric A1 acceptance gate FAILED.\n' >&2
  exit 1
fi
acceptance_source_finish "Work Fabric A1" ".claude/work-fabric-a1-accept-sha" || exit 1
