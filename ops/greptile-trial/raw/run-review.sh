#!/usr/bin/env bash
# Trial helper: run a Greptile CLI review and archive the raw output.
# Usage: run-review.sh <label> <workdir> [base-branch] [extra greptile args...]
set -uo pipefail

LABEL="${1:?need a label}"
WORKDIR="${2:?need a workdir}"
# Default to origin/main: the local main ref is routinely stale in this
# checkout, and reviewing against it reconstructs old merges in reverse.
BASE="${3:-origin/main}"
if (( $# >= 3 )); then
  shift 3
else
  shift "$#"
fi

OUT="$HOME/heiwa/ops/greptile-trial/raw"
mkdir -p "$OUT"

export GREPTILE_TELEMETRY_DISABLED=1
if [ -z "${GREPTILE_API_KEY:-}" ]; then
  echo "GREPTILE_API_KEY not set" >&2
  exit 2
fi

cd "$WORKDIR" || exit 2
echo "=== $LABEL :: $(git rev-parse --abbrev-ref HEAD) vs $BASE ==="
echo "=== diffstat ==="
git diff --stat "$BASE"...HEAD | tail -3

greptile review --branch "$BASE" --json "$@" > "$OUT/$LABEL.json" 2> "$OUT/$LABEL.err"
RC=$?
echo "=== exit $RC ==="
if [ -s "$OUT/$LABEL.err" ]; then
  echo "--- stderr ---"
  head -20 "$OUT/$LABEL.err"
fi
if [ -s "$OUT/$LABEL.json" ]; then
  echo "--- json bytes: $(wc -c < "$OUT/$LABEL.json") ---"
  head -c 2000 "$OUT/$LABEL.json"
fi
exit $RC
