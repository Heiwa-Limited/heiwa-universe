#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/heiwa-public-web.XXXXXX")"
trap 'rm -rf -- "$tmp_dir"' EXIT

"$repo_root/scripts/package_public_web.sh" "$tmp_dir/site"

required=(
  index.html
  download.html
  status.html
  domains.html
  governance.html
  support.html
  install
  _headers
  assets/site.js
  assets/status.js
  assets/domains.js
  assets/styles.css
  assets/tokens.css
  assets/instrument.css
  assets/domains.bootstrap.json
  assets/providers.json
  vs/manifest.html
)

for path in "${required[@]}"; do
  if [[ ! -f "$tmp_dir/site/$path" ]]; then
    echo "missing public artifact: $path" >&2
    exit 1
  fi
done

forbidden=(
  dashboard.html
  approvals.html
  cells.html
  connections.html
  history.html
  live.html
  missions.html
  rate-groups.html
  assets/operator.js
  assets/operator.ts
  assets/status.ts
)

for path in "${forbidden[@]}"; do
  if [[ -e "$tmp_dir/site/$path" ]]; then
    echo "private operator surface leaked into public artifact: $path" >&2
    exit 1
  fi
done

if rg -n '<script(?![^>]*\bsrc=)[^>]*>' "$tmp_dir/site" -g '*.html' -P; then
  echo "inline script violates the public Content-Security-Policy" >&2
  exit 1
fi

echo "Public web package checks passed."
