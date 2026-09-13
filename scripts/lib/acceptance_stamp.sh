#!/usr/bin/env bash
# Bind an acceptance gate's stamp to the exact source its checks observed.
#
# A stamp claims "this revision passed". That is only true when the revision
# was clean — tracked and untracked source alike — when the checks started,
# and was still the same revision, still clean, when they finished. A tree
# that becomes clean at the end, or a commit made while checks ran, would let
# a stamp describe source no check ever saw.
#
# Usage, from the repository root after `set -euo pipefail`:
#   acceptance_source_begin
#   ... run checks ...
#   acceptance_source_finish "<label>" "<stamp file>" || exit 1

acceptance_tree_is_clean() {
  [[ -z "$(git status --porcelain --untracked-files=all)" ]]
}

acceptance_source_begin() {
  ACCEPTANCE_START_HEAD="$(git rev-parse HEAD)"
  if acceptance_tree_is_clean; then
    ACCEPTANCE_START_CLEAN=1
  else
    ACCEPTANCE_START_CLEAN=0
  fi
}

acceptance_source_finish() {
  local label="$1" stamp="$2" end_head
  if [[ -z "${ACCEPTANCE_START_HEAD:-}" ]]; then
    printf '%s acceptance gate FAILED: acceptance_source_begin was not called before the checks.\n' "$label" >&2
    return 1
  fi
  end_head="$(git rev-parse HEAD)"
  if [[ "$end_head" != "$ACCEPTANCE_START_HEAD" ]]; then
    printf '%s acceptance gate FAILED: HEAD moved from %s to %s while checks ran, so the results describe neither revision.\n' \
      "$label" "$ACCEPTANCE_START_HEAD" "$end_head" >&2
    return 1
  fi
  if (( ACCEPTANCE_START_CLEAN == 0 )); then
    printf '%s acceptance gate passed. Tree was dirty when checks started, so no HEAD stamp was written.\n' "$label"
    return 0
  fi
  if ! acceptance_tree_is_clean; then
    printf '%s acceptance gate FAILED: source changed while checks ran (see git status), so %s cannot be stamped.\n' \
      "$label" "$ACCEPTANCE_START_HEAD" >&2
    return 1
  fi
  mkdir -p "$(dirname "$stamp")"
  printf '%s\n' "$ACCEPTANCE_START_HEAD" > "$stamp.tmp"
  mv "$stamp.tmp" "$stamp"
  printf '%s acceptance gate passed (stamp written for %s).\n' "$label" "$ACCEPTANCE_START_HEAD"
}
