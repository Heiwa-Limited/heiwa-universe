#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if (( $# > 0 )); then
  targets=("$@")
else
  targets=(apps/heiwa_shell crates/heiwa_loop)
fi

# Match both conventional adapter names and aliased/multiline provider sends
# whose first two arguments are model + messages. mpsc/watch sends take one
# argument and therefore do not trip the second arm.
pattern='(?s)(?:\b[A-Za-z_][A-Za-z0-9_]*adapter[A-Za-z0-9_]*\b)\s*\.\s*send\s*\(|\.\s*send\s*\(\s*&[A-Za-z_][A-Za-z0-9_]*\s*,\s*&?\s*(?:messages[A-Za-z0-9_]*|[A-Za-z_][A-Za-z0-9_]*_messages[A-Za-z0-9_]*)\b'

violations="$({
  rg --files-with-matches --line-number --multiline --pcre2 "$pattern" "${targets[@]}" \
    --glob '*.rs' 2>/dev/null || true
} | while IFS= read -r path; do
  [[ -z "$path" ]] && continue
  [[ "$path" == "apps/heiwa_shell/src/model_calls.rs" ]] && continue
  printf '%s\n' "$path"
done)"

if [[ -n "$violations" ]]; then
  printf 'model_call_boundary=failed\n' >&2
  printf '%s\n' "$violations" >&2
  exit 1
fi

printf 'model_call_boundary=ok\n'
