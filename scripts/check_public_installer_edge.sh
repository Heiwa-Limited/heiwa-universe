#!/usr/bin/env bash
set -euo pipefail

installer_url="${HEIWA_PUBLIC_INSTALLER_URL:-https://heiwa.ltd/install}"
attempts="${HEIWA_PUBLIC_INSTALLER_ATTEMPTS:-4}"

if [[ ! "$attempts" =~ ^[1-9][0-9]*$ ]]; then
  echo "HEIWA_PUBLIC_INSTALLER_ATTEMPTS must be a positive integer." >&2
  exit 2
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/heiwa-installer-check.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

for attempt in $(seq 1 "$attempts"); do
  headers="$tmp_dir/headers-$attempt"
  body="$tmp_dir/install-$attempt"
  status="$({
    curl --proto '=https' --tlsv1.2 --location --silent --show-error \
      --retry 2 --retry-all-errors --retry-delay 1 \
      --user-agent 'Heiwa-Installer/0.1 (+https://heiwa.ltd)' \
      --header 'Accept: text/x-shellscript' \
      --dump-header "$headers" \
      --output "$body" \
      --write-out '%{http_code}' \
      "$installer_url"
  } || true)"

  if [[ "$status" == "200" ]] \
    && ! grep -Eiq '^cf-mitigated:[[:space:]]*challenge' "$headers" \
    && head -n 1 "$body" | grep -qx '#!/bin/sh'; then
    echo "Public installer edge check passed: $installer_url"
    exit 0
  fi

  echo "Public installer edge check attempt $attempt/$attempts failed (HTTP ${status:-000})." >&2
  grep -Ei '^(HTTP/|cf-ray|cf-mitigated|content-type|server):' "$headers" >&2 || true
  if [[ "$attempt" -lt "$attempts" ]]; then
    sleep 2
  fi
done

exit 1
