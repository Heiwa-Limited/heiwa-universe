#!/usr/bin/env bash
# Keep the public installer's fallback version equal to the release being cut.
#
# The installer resolves the newest release at run time, but falls back to a
# literal pin when that lookup fails. Before this gate existed the pin was the
# ONLY source of the version, and nothing kept it fresh: tagging v0.1.1 would
# have left `curl https://heiwa.ltd/install | sh` silently installing v0.1.0 —
# no error, just an old binary for every new user.
#
# release.yml runs this with the tag being published, so a stale pin fails the
# release instead of shipping a stale front door.
#
# Usage: scripts/check_installer_version_pin.sh <version>   # e.g. 0.1.1
#        scripts/check_installer_version_pin.sh             # verify mirror only
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

installer="apps/heiwa_app/clients/web/install"
mirror="apps/heiwa_app/clients/web/install.sh"

for path in "$installer" "$mirror"; do
  [[ -f "$path" ]] || { echo "missing public installer: $path" >&2; exit 1; }
done

cmp -s "$installer" "$mirror" || {
  echo "install and install.sh must be byte-identical" >&2
  exit 1
}

pinned="$(sed -n 's/^pinned_version="\([0-9][0-9.]*\)"$/\1/p' "$installer")"
if [[ -z "$pinned" ]]; then
  echo "could not read pinned_version from $installer" >&2
  exit 1
fi

if [[ $# -eq 0 ]]; then
  echo "Public installer fallback pin: $pinned (no release version supplied to compare)."
  exit 0
fi

release="${1#v}"
if [[ ! "$release" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "release version must be a stable semantic version such as 0.1.1: $1" >&2
  exit 1
fi

if [[ "$pinned" != "$release" ]]; then
  cat >&2 <<EOF
public installer fallback pin is stale
  releasing : $release
  pinned    : $pinned
Update pinned_version in both $installer and $mirror, then re-run the release.
EOF
  exit 1
fi

echo "Public installer fallback pin matches the release ($release)."
