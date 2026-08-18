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
  '# CURL_SCENARIO may name one scenario or a sequence of them; with' \
  '# CURL_COUNTER set, each invocation advances to the next and the last one' \
  '# repeats once the sequence is exhausted.' \
  'read -r -a scenarios <<<"${CURL_SCENARIO:?}"' \
  'index=0' \
  'if [[ -n "${CURL_COUNTER:-}" ]]; then' \
  '  index="$(cat "$CURL_COUNTER" 2>/dev/null || printf 0)"' \
  '  printf "%s" "$((index + 1))" >"$CURL_COUNTER"' \
  '  if (( index >= ${#scenarios[@]} )); then' \
  '    index=$(( ${#scenarios[@]} - 1 ))' \
  '  fi' \
  'fi' \
  'case "${scenarios[index]}" in' \
  '  success)' \
  '    printf "HTTP/2 200\\ncontent-type: text/x-shellscript\\n\\n" >"$headers"' \
  '    cat "${REPO_INSTALLER:?}" >"$body"' \
  '    printf 200' \
  '    ;;' \
  '  drift)' \
  '    printf "HTTP/2 200\\ncontent-type: text/x-shellscript\\n\\n" >"$headers"' \
  '    printf "#!/bin/sh\\nset -eu\\n# a stale deploy\\n" >"$body"' \
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
    REPO_INSTALLER="$repo_root/apps/heiwa_app/clients/web/install" \
    HEIWA_PUBLIC_INSTALLER_ATTEMPTS=1 \
    PATH="$tmp_dir/bin:$PATH" \
    bash "$checker" >"$tmp_dir/output" 2>&1
}

run_check_sequence() {
  local counter="$tmp_dir/counter"
  printf 0 >"$counter"
  CURL_SCENARIO="$1" \
    CURL_COUNTER="$counter" \
    REPO_INSTALLER="$repo_root/apps/heiwa_app/clients/web/install" \
    HEIWA_PUBLIC_INSTALLER_ATTEMPTS="$2" \
    HEIWA_PUBLIC_INSTALLER_RETRY_DELAY=0 \
    PATH="$tmp_dir/bin:$PATH" \
    bash "$checker" >"$tmp_dir/output" 2>&1
}

run_check_with_drift_allowed() {
  CURL_SCENARIO="$1" \
    REPO_INSTALLER="$repo_root/apps/heiwa_app/clients/web/install" \
    HEIWA_ALLOW_INSTALLER_EDGE_DRIFT=1 \
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
# A 200 that is shaped like a shell script but is not the installer we ship is
# exactly the failure that kept heiwa.ltd on a pinned version while every other
# check stayed green.
if run_check drift; then
  echo "expected a stale edge installer to fail" >&2
  exit 1
fi
if ! grep -q "does not match the repo copy" "$tmp_dir/output"; then
  cat "$tmp_dir/output" >&2
  echo "expected drift failure to name the mismatch" >&2
  exit 1
fi
if ! run_check_with_drift_allowed drift; then
  cat "$tmp_dir/output" >&2
  echo "expected the deploy-window override to allow drift" >&2
  exit 1
fi
# Drift on an early attempt followed by a response we cannot compare is an edge
# availability failure, not a stale installer: reporting the earlier body as
# persistent drift would diff a copy the edge is no longer serving.
if run_check_sequence "drift challenge" 2; then
  echo "expected a trailing Cloudflare challenge to fail" >&2
  exit 1
fi
if grep -q "does not match the repo copy" "$tmp_dir/output"; then
  cat "$tmp_dir/output" >&2
  echo "expected a trailing challenge to be reported as a failed attempt," \
    "not as persistent installer drift" >&2
  exit 1
fi
if ! grep -q "attempt 2/2 failed (HTTP 403)" "$tmp_dir/output"; then
  cat "$tmp_dir/output" >&2
  echo "expected the final challenge attempt to be reported" >&2
  exit 1
fi
# The reverse order still reports drift: the last comparable answer is the one
# that decides, and here it is a stale installer.
if run_check_sequence "challenge drift" 2; then
  echo "expected a trailing stale installer to fail" >&2
  exit 1
fi
if ! grep -q "does not match the repo copy" "$tmp_dir/output"; then
  cat "$tmp_dir/output" >&2
  echo "expected a trailing stale installer to be reported as drift" >&2
  exit 1
fi

echo "Public installer edge checker tests passed."
