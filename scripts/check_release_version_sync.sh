#!/usr/bin/env bash
# Assert every declaration describing the shipped artifact's version matches
# the release being cut.
#
# v0.2.0 shipped with all of them still reading 0.1.0. The binary reported
# `heiwa 0.1.0`, and `heiwa app update` computes update_available as
# `latest != current` -- so a freshly installed v0.2.0 offered an update to
# the version it was already running, permanently, and the desktop update
# banner would never clear. Nothing caught it because every gate compared the
# tag against the installer pin and none compared it against the code.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${1:-}"
if [[ -z "$version" ]]; then
  echo "usage: $(basename "$0") <version>   # e.g. 0.2.0" >&2
  exit 2
fi
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "version must look like 0.2.0, got: $version" >&2
  exit 2
fi

fail=0

check_toml() {
  local file="$1"
  local found
  found="$(awk '/^\[package\]/{p=1;next} /^\[/{p=0} p && /^version *=/{gsub(/[" ]/,"");sub(/^version=/,"");print;exit}' "$file")"
  if [[ "$found" != "$version" ]]; then
    echo "$file declares version $found, release is $version" >&2
    fail=1
  fi
}

check_json() {
  local file="$1"
  local found
  found="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("version",""))' "$file")"
  if [[ "$found" != "$version" ]]; then
    echo "$file declares version $found, release is $version" >&2
    fail=1
  fi
}

# The runtime binary users run, the core it embeds, and the three that decide
# what the desktop shell reports to the updater.
check_toml apps/heiwa_shell/Cargo.toml
check_toml apps/heiwa_core/Cargo.toml
check_toml apps/heiwa_app/desktop/src-tauri/Cargo.toml
check_json apps/heiwa_app/desktop/src-tauri/tauri.conf.json
check_json apps/heiwa_app/desktop/package.json

if [[ "$fail" -ne 0 ]]; then
  echo "Bump these to $version before releasing; a shipped artifact that" \
    "misreports its own version offers itself an endless update." >&2
  exit 1
fi

echo "Release version declarations all match $version."
