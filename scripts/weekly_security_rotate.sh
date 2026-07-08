#!/usr/bin/env bash
set -u -o pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_ROOT="${HEIWA_SECURITY_REPORT_DIR:-$HOME/.heiwa/state/security}"
ROTATION_DIR="$REPORT_ROOT/rotations"
RUN_ID="$(date -u '+%Y%m%dT%H%M%SZ')"
REPORT="$ROTATION_DIR/$RUN_ID.log"
LATEST="$REPORT_ROOT/latest-weekly-security-rotation.log"

export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"

mkdir -p "$ROTATION_DIR"
chmod 700 "$REPORT_ROOT" "$ROTATION_DIR" 2>/dev/null || true

status=0
{
  printf 'heiwa weekly security rotation\n'
  printf 'run_id: %s\n' "$RUN_ID"
  printf 'root: %s\n' "$ROOT"
  printf 'started_at_utc: %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'host: %s\n' "$(hostname)"
  printf 'user: %s\n' "$(id -un)"

  printf '\n## machine security check/fix\n'
  bash "$ROOT/scripts/check_machine_security.sh" --fix || status=$?

  printf '\n## repo security gate\n'
  bash "$ROOT/scripts/verify_security.sh" || status=$?

  printf '\nfinished_at_utc: %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'exit_status: %s\n' "$status"
} >"$REPORT" 2>&1

chmod 600 "$REPORT" 2>/dev/null || true
ln -sfn "$REPORT" "$LATEST"
find "$ROTATION_DIR" -type f -name '*.log' -mtime +90 -delete 2>/dev/null || true

fail_line="$(grep -E 'failures: [1-9][0-9]*' "$REPORT" | tail -1 || true)"
warn_line="$(grep -E 'warnings: [0-9]+' "$REPORT" | tail -1 || true)"
gitleaks_line="$(grep -E 'WRN leaks found:' "$REPORT" | tail -1 || true)"

if [ "$status" -eq 0 ] && [ -z "$fail_line" ]; then
  printf 'heiwa weekly security rotation: PASS\n'
else
  printf 'heiwa weekly security rotation: FAIL\n'
fi
printf 'report: %s\n' "$REPORT"
[ -n "$warn_line" ] && printf '%s\n' "$warn_line"
[ -n "$gitleaks_line" ] && printf '%s\n' "$gitleaks_line"

exit "$status"
