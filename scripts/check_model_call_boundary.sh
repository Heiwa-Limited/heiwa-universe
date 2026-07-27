#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

allowed_path='apps/heiwa_shell/src/model_calls.rs'
provider_adapter_prefix='crates/heiwa_provider/src/providers/'

# ProviderAdapter::send has three arguments after method dispatch: model,
# message history by reference, and stream sender. Match that shape regardless
# of receiver/variable names and formatting. Also catch trait UFCS dispatch.
adapter_send_pattern='(?s)\.\s*send\s*\(\s*&?\s*[A-Za-z_][A-Za-z0-9_]*(?:\s*\.\s*[A-Za-z_][A-Za-z0-9_]*(?:\s*\(\s*\))?)*\s*,\s*(?:&\s*[A-Za-z_][A-Za-z0-9_]*|[A-Za-z_][A-Za-z0-9_]*(?:\s*\.\s*[A-Za-z_][A-Za-z0-9_]*(?:\s*\(\s*\))?)+)\s*,|(?:\b[A-Za-z_][A-Za-z0-9_]*::)*\bProviderAdapter\s*::\s*send\s*\('
# Production inference must not bypass ModelCallExecutor through a provider
# endpoint or by spawning a provider-owned CLI in the shell/loop crates.
direct_inference_pattern='(?s)/api/(?:generate|chat)|/v1/chat(?:/completions)?|\b(?:std::process::|tokio::process::)?Command\s*::\s*new\s*\(\s*"(?:ollama|claude|codex|gemini|grok)"|\breqwest\b.{0,1200}(?:/api/(?:generate|chat)|/v1/chat(?:/completions)?)|\bCommand\s*::\s*new\s*\(\s*"curl"\s*\).{0,1200}(?:/api/(?:generate|chat)|/v1/chat(?:/completions)?)'

matches_pattern() {
  printf '%s\n' "$1" | rg --quiet --multiline --pcre2 "$adapter_send_pattern"
}

matches_direct_inference() {
  printf '%s\n' "$1" | rg --quiet --multiline --pcre2 "$direct_inference_pattern"
}

is_allowed_path() {
  [[ "$1" == "$allowed_path" ]]
}

is_provider_adapter_path() {
  [[ "$1" == "$provider_adapter_prefix"*.rs ]]
}

content_is_violation() {
  local path="$1" content="$2"
  matches_direct_inference "$content" && ! is_provider_adapter_path "$path" && return 0
  matches_pattern "$content" && ! is_allowed_path "$path"
}

self_test() {
  local allowed_fixture alias_fixture ufcs_fixture expression_model_fixture expression_messages_fixture safe_fixture endpoint_fixture cli_fixture curl_fixture
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
  endpoint_fixture='client.post("http://localhost:11434/api/generate")'
  inventory_fixture='client.get("http://localhost:11434/api/tags")'
  cli_fixture='tokio::process::Command::new("ollama").arg("run")'
  curl_fixture='Command::new("curl").arg("http://localhost:11434/v1/chat")'

  matches_pattern "$allowed_fixture" || return 1
  matches_pattern "$alias_fixture" || return 1
  matches_pattern "$ufcs_fixture" || return 1
  matches_pattern "$expression_model_fixture" || return 1
  matches_pattern "$expression_messages_fixture" || return 1
  ! matches_pattern "$safe_fixture" || return 1
  matches_direct_inference "$endpoint_fixture" || return 1
  ! matches_direct_inference "$inventory_fixture" || return 1
  matches_direct_inference "$cli_fixture" || return 1
  matches_direct_inference "$curl_fixture" || return 1
  ! matches_direct_inference "$safe_fixture" || return 1
  is_allowed_path "$allowed_path" || return 1
  ! is_allowed_path 'apps/heiwa_shell/src/other.rs' || return 1
  ! content_is_violation "$allowed_path" "$allowed_fixture" || return 1
  content_is_violation "$allowed_path" "$endpoint_fixture" || return 1
  ! content_is_violation 'crates/heiwa_provider/src/providers/ollama.rs' "$cli_fixture" || return 1
  content_is_violation 'apps/heiwa_shell/src/other.rs' "$allowed_fixture" || return 1
}

if ! self_test; then
  printf 'model_call_boundary=self_test_failed\n' >&2
  exit 1
fi

if (( $# > 0 )); then
  targets=("$@")
else
  targets=(apps/heiwa_shell crates/heiwa_loop crates/heiwa_provider)
fi

violations="$({
  rg --files-with-matches --line-number --multiline --pcre2 \
    -e "$adapter_send_pattern" -e "$direct_inference_pattern" "${targets[@]}" \
    --glob '*.rs' --glob '!**/tests/**' 2>/dev/null || true
} | while IFS= read -r path; do
  [[ -z "$path" ]] && continue
  if ! is_provider_adapter_path "$path" \
    && rg --quiet --multiline --pcre2 "$direct_inference_pattern" "$path"; then
    printf '%s\n' "$path"
  elif ! is_allowed_path "$path" \
    && rg --quiet --multiline --pcre2 "$adapter_send_pattern" "$path"; then
    printf '%s\n' "$path"
  fi
done)"

if [[ -n "$violations" ]]; then
  printf 'model_call_boundary=failed\n' >&2
  printf '%s\n' "$violations" >&2
  exit 1
fi

printf 'model_call_boundary=ok\n'
