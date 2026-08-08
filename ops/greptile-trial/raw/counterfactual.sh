#!/usr/bin/env bash
# Counterfactual helper: have a cold, non-authoring provider review the same
# diff Greptile reviewed, so the two can be compared.
#
# Usage: counterfactual.sh <label> <head-ref> [base-ref]
#   e.g. counterfactual.sh 52 origin/dev origin/main
#
# The provider must not have seen Greptile's comments on this PR. `codex exec
# review --base` refuses a custom prompt, so it runs on Codex's own defaults --
# no steer, no repo context. That is deliberate: it is the coldest form, and it
# disadvantages the free alternative rather than flattering it.
set -uo pipefail

LABEL="${1:?need a label, e.g. 52}"
HEAD_REF="${2:?need a head ref, e.g. origin/dev}"
BASE_REF="${3:-origin/main}"

W="$HOME/heiwa/.worktrees/claude/cf-$LABEL"
OUT="$HOME/heiwa/ops/greptile-trial/raw"
mkdir -p "$OUT"

cd "$HOME/heiwa" || exit 2
git fetch origin --quiet
if [ ! -d "$W" ]; then
  git worktree add --detach "$W" "$HEAD_REF" >/dev/null 2>&1 || {
    echo "worktree add failed for $HEAD_REF" >&2; exit 2; }
fi

cd "$W" || exit 2
echo "=== $LABEL :: $(git rev-parse --short HEAD) vs $BASE_REF ==="
git diff --stat "$BASE_REF"...HEAD | tail -2

codex exec review --base "$BASE_REF" \
  > "$OUT/counterfactual-codex-$LABEL.txt" 2>"$OUT/counterfactual-codex-$LABEL.err"
RC=$?

echo "=== exit $RC ==="
if [ -s "$OUT/counterfactual-codex-$LABEL.txt" ]; then
  cat "$OUT/counterfactual-codex-$LABEL.txt"
else
  echo "no output; stderr tail:" >&2
  tail -15 "$OUT/counterfactual-codex-$LABEL.err" >&2
fi

echo
echo "Now compare against Greptile's comments on the PR:"
echo "  gh api repos/Heiwa-Limited/heiwa-universe/pulls/$LABEL/comments --jq '.[] | \"\\(.path):\\(.line)\"'"
echo "Record the three-way table in ops/greptile-trial/raw/counterfactual.md."
echo "Verify every finding against the source before logging it -- both tools."
exit $RC
