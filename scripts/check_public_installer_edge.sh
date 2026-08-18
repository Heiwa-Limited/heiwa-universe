#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
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

    # Reaching a 200 that is shaped like a shell script proved only that the
    # edge answers. It never proved the edge serves *this* installer. That gap
    # let heiwa.ltd keep serving a version-pinned copy while the repo's
    # installer -- the one check_public_installer.sh validates -- resolved the
    # newest release. Both checks passed; every published release stayed
    # invisible to `curl | sh`.
    repo_installer="$repo_root/apps/heiwa_app/clients/web/install"
    if [[ ! -f "$repo_installer" ]]; then
      echo "Repo installer missing at $repo_installer" >&2
      exit 2
    fi

    if cmp -s "$repo_installer" "$body"; then
      echo "Public installer edge check passed: $installer_url"
      exit 0
    fi

    if [[ "${HEIWA_ALLOW_INSTALLER_EDGE_DRIFT:-0}" == "1" ]]; then
      echo "Edge installer differs from the repo; continuing because" \
        "HEIWA_ALLOW_INSTALLER_EDGE_DRIFT=1." >&2
      exit 0
    fi

    echo "Edge installer at $installer_url does not match the repo copy." >&2
    echo "The edge is serving a stale deploy. Redeploy" \
      "apps/heiwa_app/clients/web/install, or set" \
      "HEIWA_ALLOW_INSTALLER_EDGE_DRIFT=1 during a deploy window." >&2
    echo "--- diff (repo vs edge) ---" >&2
    diff "$repo_installer" "$body" >&2 || true
    exit 1
  fi

  echo "Public installer edge check attempt $attempt/$attempts failed (HTTP ${status:-000})." >&2
  grep -Ei '^(HTTP/|cf-ray|cf-mitigated|content-type|server):' "$headers" >&2 || true
  if [[ "$attempt" -lt "$attempts" ]]; then
    sleep 2
  fi
done

exit 1
