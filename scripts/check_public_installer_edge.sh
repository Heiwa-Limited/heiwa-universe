#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
installer_url="${HEIWA_PUBLIC_INSTALLER_URL:-https://heiwa.ltd/install}"
attempts="${HEIWA_PUBLIC_INSTALLER_ATTEMPTS:-4}"
# Cloudflare propagation after a deploy takes longer than a network blip, so
# the gap between attempts is tunable independently of the attempt count.
retry_delay="${HEIWA_PUBLIC_INSTALLER_RETRY_DELAY:-2}"

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

    # Drift is retried, not failed on immediately. Run straight after a
    # `wrangler pages deploy` the edge has not finished propagating, and an
    # instant verdict reports a stale deploy that is about to correct itself
    # -- which is exactly what happened on run 32180221136, where the deploy
    # succeeded and this check failed the job seconds later.
    drift_body="$body"
    echo "Edge installer differs from the repo (attempt $attempt/$attempts);" \
      "retrying in case a deploy is still propagating." >&2
  else
    echo "Public installer edge check attempt $attempt/$attempts failed (HTTP ${status:-000})." >&2
    grep -Ei '^(HTTP/|cf-ray|cf-mitigated|content-type|server):' "$headers" >&2 || true
  fi

  if [[ "$attempt" -lt "$attempts" ]]; then
    sleep "$retry_delay"
  fi
done

if [[ -n "${drift_body:-}" ]]; then
  echo "Edge installer at $installer_url still does not match the repo copy" \
    "after $attempts attempts." >&2
  echo "Redeploy apps/heiwa_app/clients/web/install, or set" \
    "HEIWA_ALLOW_INSTALLER_EDGE_DRIFT=1 during a deploy window." >&2
  echo "--- diff (repo vs edge) ---" >&2
  diff "$repo_installer" "$drift_body" >&2 || true
fi

exit 1
