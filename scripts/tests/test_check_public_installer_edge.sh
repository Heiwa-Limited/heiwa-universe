#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
checker="$repo_root/scripts/check_public_installer_edge.sh"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/heiwa-installer-check-test.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

mkdir -p "$tmp_dir/bin"
curl_stub="$tmp_dir/bin/curl"
# The single-quoted strings intentionally write literal shell variables into
# the generated curl stub instead of expanding them in this parent process.
# shellcheck disable=SC2016
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'headers=""' \
  'body=""' \
  'while [[ $# -gt 0 ]]; do' \
  '  case "$1" in' \
  '    --dump-header) headers="$2"; shift 2 ;;' \
  '    --output) body="$2"; shift 2 ;;' \
  '    --write-out|--proto|--retry|--retry-delay|--user-agent|--header) shift 2 ;;' \
  '    *) shift ;;' \
  '  esac' \
  'done' \
  'case "${CURL_SCENARIO:?}" in' \
  '  success)' \
  '    printf "HTTP/2 200\\ncontent-type: text/x-shellscript\\n\\n" >"$headers"' \
  '    printf "#!/bin/sh\\nset -eu\\n" >"$body"' \
  '    printf 200' \
  '    ;;' \
  '  challenge)' \
  '    printf "HTTP/2 403\\ncf-mitigated: challenge\\ncontent-type: text/html\\n\\n" >"$headers"' \
  '    printf "<!doctype html>\\n" >"$body"' \
  '    printf 403' \
  '    ;;' \
  '  html)' \
  '    printf "HTTP/2 200\\ncontent-type: text/html\\n\\n" >"$headers"' \
  '    printf "<!doctype html>\\n" >"$body"' \
  '    printf 200' \
  '    ;;' \
  'esac' >"$curl_stub"
chmod +x "$curl_stub"

run_check() {
  CURL_SCENARIO="$1" \
    HEIWA_PUBLIC_INSTALLER_ATTEMPTS=1 \
    PATH="$tmp_dir/bin:$PATH" \
    bash "$checker" >"$tmp_dir/output" 2>&1
}

if ! run_check success; then
  cat "$tmp_dir/output" >&2
  echo "expected a shell installer response to pass" >&2
  exit 1
fi
if run_check challenge; then
  echo "expected a Cloudflare challenge response to fail" >&2
  exit 1
fi
if run_check html; then
  echo "expected a 200 HTML response to fail" >&2
  exit 1
fi

echo "Public installer edge checker tests passed."
