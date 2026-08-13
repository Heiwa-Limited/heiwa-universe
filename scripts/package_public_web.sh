#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <empty-output-directory>" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_root="$repo_root/apps/heiwa_app/clients/web"
destination_arg="$1"

if [[ -z "$destination_arg" || "$destination_arg" == "/" || "$destination_arg" == "." ]]; then
  echo "refusing unsafe public-web output path: $destination_arg" >&2
  exit 2
fi

mkdir -p "$(dirname "$destination_arg")"
destination="$(cd "$(dirname "$destination_arg")" && pwd)/$(basename "$destination_arg")"
if [[ "$destination" == "$repo_root" || "$destination" == "$source_root" ]]; then
  echo "refusing to package over source or repository root: $destination" >&2
  exit 2
fi

mkdir -p "$destination"
if [[ -n "$(find "$destination" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
  echo "public-web output directory must be empty: $destination" >&2
  exit 2
fi

root_files=(
  _headers
  _redirects
  domains.html
  download.html
  favicon.svg
  governance.html
  index.html
  install
  install.sh
  providers.html
  robots.txt
  status.html
  support.html
  tokens.json
)
asset_files=(
  domains.bootstrap.json
  domains.js
  instrument.css
  providers.json
  site.js
  status.js
  styles.css
  tokens.css
)
comparison_files=(litellm.html manifest.html openrouter.html)

mkdir -p "$destination/assets" "$destination/vs"
for path in "${root_files[@]}"; do
  install -m 0644 "$source_root/$path" "$destination/$path"
done
for path in "${asset_files[@]}"; do
  install -m 0644 "$source_root/assets/$path" "$destination/assets/$path"
done
for path in "${comparison_files[@]}"; do
  install -m 0644 "$source_root/vs/$path" "$destination/vs/$path"
done

echo "Packaged public web allowlist at $destination"
