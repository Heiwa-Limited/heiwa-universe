#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

allowed_path='apps/heiwa_shell/src/model_calls.rs'

# ProviderAdapter::send has three arguments after method dispatch: model,
# message history by reference, and stream sender. Match that shape regardless
# of receiver/variable names and formatting. Also catch trait UFCS dispatch.
pattern='(?s)\.\s*send\s*\(\s*&?\s*[A-Za-z_][A-Za-z0-9_]*(?:\s*\.\s*[A-Za-z_][A-Za-z0-9_]*(?:\s*\(\s*\))?)*\s*,\s*(?:&\s*[A-Za-z_][A-Za-z0-9_]*|[A-Za-z_][A-Za-z0-9_]*(?:\s*\.\s*[A-Za-z_][A-Za-z0-9_]*(?:\s*\(\s*\))?)+)\s*,|(?:\b[A-Za-z_][A-Za-z0-9_]*::)*\bProviderAdapter\s*::\s*send\s*\('

matches_pattern() {
  printf '%s\n' "$1" | rg --quiet --multiline --pcre2 "$pattern"
}

is_allowed_path() {
  [[ "$1" == "$allowed_path" ]]
}

self_test() {
  local allowed_fixture alias_fixture ufcs_fixture expression_model_fixture expression_messages_fixture safe_fixture
  allowed_fixture='adapter.send(model, &messages, stream_tx)'
  alias_fixture='gateway
    .send(
        model,
        &history,
        stream_tx,
    )'
  ufcs_fixture='heiwa_provider::adapter::ProviderAdapter::send(
      adapter,
      model,
      &history,
      stream_tx,
  )'
  expression_model_fixture='gateway.send(model.as_str(), &history, stream_tx)'
  expression_messages_fixture='gateway.send(model, messages.as_slice(), stream_tx)'
  safe_fixture='event_tx.send(StreamEvent::Done(usage))'

  matches_pattern "$allowed_fixture" || return 1
  matches_pattern "$alias_fixture" || return 1
  matches_pattern "$ufcs_fixture" || return 1
  matches_pattern "$expression_model_fixture" || return 1
  matches_pattern "$expression_messages_fixture" || return 1
  ! matches_pattern "$safe_fixture" || return 1
  is_allowed_path "$allowed_path" || return 1
  ! is_allowed_path 'apps/heiwa_shell/src/other.rs' || return 1
}

if ! self_test; then
  printf 'model_call_boundary=self_test_failed\n' >&2
  exit 1
fi

if (( $# > 0 )); then
  targets=("$@")
else
  targets=(apps/heiwa_shell crates/heiwa_loop)
fi

violations="$({
  rg --files-with-matches --line-number --multiline --pcre2 "$pattern" "${targets[@]}" \
    --glob '*.rs' 2>/dev/null || true
} | while IFS= read -r path; do
  [[ -z "$path" ]] && continue
  is_allowed_path "$path" && continue
  printf '%s\n' "$path"
done)"

if [[ -n "$violations" ]]; then
  printf 'model_call_boundary=failed\n' >&2
  printf '%s\n' "$violations" >&2
  exit 1
fi

printf 'model_call_boundary=ok\n'
