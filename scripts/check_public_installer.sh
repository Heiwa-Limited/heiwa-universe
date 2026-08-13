#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

installer="apps/heiwa_app/clients/web/install"
mirror="apps/heiwa_app/clients/web/install.sh"

cmp -s "$installer" "$mirror" || {
  echo "install and install.sh must be byte-identical" >&2
  exit 1
}

required=(
  'repo="Heiwa-Limited/heiwa-universe"'
  '/releases/download/v'
  'checksums.txt'
  'sha256sum'
  'shasum -a 256'
  'mktemp -d'
  'HEIWA_VERSION'
  'pinned_version='
  'resolve_latest_version'
  'HEIWA_HOME must be an absolute path'
  'mv -f'
  'cockpit-current'
  'archive contains links or unsupported entry types'
  'mv -fh'
  'mv -Tf'
)
for text in "${required[@]}"; do
  if ! grep -Fq "$text" "$installer"; then
    echo "public installer missing required integrity behavior: $text" >&2
    exit 1
  fi
done

for forbidden in HEIWA_PRIVATE_TOKEN 'cargo install' 'git clone' 'checkout --force'; do
  if grep -Fq "$forbidden" "$installer"; then
    echo "public installer still contains private/source-build behavior: $forbidden" >&2
    exit 1
  fi
done

shellcheck -s sh "$installer"
echo "Public release installer checks passed."
