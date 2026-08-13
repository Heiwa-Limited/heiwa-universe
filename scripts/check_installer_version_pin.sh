#!/usr/bin/env bash
# Keep the public installer's fallback version equal to the release being cut,
# in the checkout AND on the edge that actually serves it.
#
# The installer resolves the newest release at run time, but falls back to a
# literal pin when that lookup fails. Before this gate existed the pin was the
# ONLY source of the version, and nothing kept it fresh: tagging v0.1.1 would
# have left `curl https://heiwa.ltd/install | sh` silently installing v0.1.0 —
# no error, just an old binary for every new user.
#
# Two modes, because the checkout and the served bytes are different truths:
#
#   repo    — the pin in this checkout matches the release being cut
#   served  — the pin in the script actually served at heiwa.ltd/install
#             matches it too
#
# The served check matters because the public installer reaches users through a
# separate, dispatch-only Cloudflare deploy that the release workflow does not
# trigger. A green repo pin proves nothing about what the edge is handing out,
# so the release requires the deploy to have happened first.
#
# Usage:
#   scripts/check_installer_version_pin.sh <version>            # checkout pin
#   scripts/check_installer_version_pin.sh --served <version>   # served pin
#   scripts/check_installer_version_pin.sh                      # report only
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

installer="apps/heiwa_app/clients/web/install"
mirror="apps/heiwa_app/clients/web/install.sh"
installer_url="${HEIWA_PUBLIC_INSTALLER_URL:-https://heiwa.ltd/install}"

read_pin() {
  sed -n 's/^pinned_version="\([0-9][0-9.]*\)"$/\1/p' "$1" | head -n 1
}

normalize_version() {
  local value="${1#v}"
  if [[ ! "$value" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "version must be a stable semantic version such as 0.1.1: $1" >&2
    return 1
  fi
  printf '%s\n' "$value"
}

mode="repo"
if [[ "${1:-}" == "--served" ]]; then
  mode="served"
  shift
fi

if [[ "$mode" == "repo" ]]; then
  for path in "$installer" "$mirror"; do
    [[ -f "$path" ]] || { echo "missing public installer: $path" >&2; exit 1; }
  done

  cmp -s "$installer" "$mirror" || {
    echo "install and install.sh must be byte-identical" >&2
    exit 1
  }

  pinned="$(read_pin "$installer")"
  if [[ -z "$pinned" ]]; then
    echo "could not read pinned_version from $installer" >&2
    exit 1
  fi

  if [[ $# -eq 0 ]]; then
    echo "Public installer fallback pin: $pinned (no release version supplied to compare)."
    exit 0
  fi

  release="$(normalize_version "$1")"
  if [[ "$pinned" != "$release" ]]; then
    cat >&2 <<EOF
public installer fallback pin is stale in the checkout
  releasing : $release
  pinned    : $pinned
Update pinned_version in both $installer and $mirror, then re-run the release.
EOF
    exit 1
  fi
  echo "Public installer fallback pin matches the release ($release)."
  exit 0
fi

# served mode
if [[ $# -eq 0 ]]; then
  echo "usage: scripts/check_installer_version_pin.sh --served <version>" >&2
  exit 2
fi
release="$(normalize_version "$1")"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/heiwa-installer-pin.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT
body="$tmp_dir/install"

status="$({
  curl --proto '=https' --tlsv1.2 --location --silent --show-error \
    --retry 2 --retry-all-errors --retry-delay 1 \
    --user-agent 'Heiwa-Installer/0.1 (+https://heiwa.ltd)' \
    --header 'Accept: text/x-shellscript' \
    --output "$body" \
    --write-out '%{http_code}' \
    "$installer_url"
} || true)"

if [[ "$status" != "200" ]]; then
  echo "could not fetch the served installer at $installer_url (HTTP ${status:-000})" >&2
  exit 1
fi

served="$(read_pin "$body")"
if [[ -z "$served" ]]; then
  echo "served installer at $installer_url carries no pinned_version" >&2
  exit 1
fi

if [[ "$served" != "$release" ]]; then
  cat >&2 <<EOF
served public installer is stale
  releasing : $release
  served    : $served  ($installer_url)
The public installer reaches users through the dispatch-only Cloudflare deploy,
which the release workflow does not trigger. Dispatch the deploy workflow so the
edge serves the $release installer, then re-run the release.
EOF
  exit 1
fi

echo "Served public installer pin matches the release ($release at $installer_url)."
