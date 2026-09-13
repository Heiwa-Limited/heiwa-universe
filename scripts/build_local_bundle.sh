#!/usr/bin/env bash
# Build what an installed Heiwa needs from this checkout, the way a release
# does, into one directory that `heiwa app update` installs on the dev channel:
#
#   <out>/cockpit/                 cockpit web assets
#   <out>/heiwa                    runtime CLI (release profile, Lance)
#   <out>/heiwa-apple-resources    EventKit helper (macOS)
#   <out>/Heiwa.app                desktop bundle carrying the same runtime (macOS)
#
# The desktop app installs its bundled runtime when it opens, so on macOS the
# runtime is taken from the bundle rather than built separately: an updated
# CLI beside an older app would be downgraded on the next launch.
#
# This script builds only. It does not install, sign for distribution, or
# restart anything. HEIWA_BUILD_CHANNEL and HEIWA_BUILD_COMMIT, when set, are
# compiled into the runtime so it can report where it came from.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: scripts/build_local_bundle.sh <output-dir>" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$1"
out="$(cd "$1" && pwd)"
cd "$repo_root"
target_root="${CARGO_TARGET_DIR:-$repo_root/target}"

echo "==> cockpit"
npm ci --ignore-scripts
npm --prefix apps/heiwa_app/clients/cockpit run build
test -f apps/heiwa_app/clients/cockpit/dist/index.html
rm -rf "$out/cockpit"
cp -R apps/heiwa_app/clients/cockpit/dist "$out/cockpit"

if [[ "$(uname -s)" == "Darwin" ]]; then
  echo "==> desktop bundle with runtime and EventKit helper"
  npm ci --ignore-scripts --prefix apps/heiwa_app/desktop
  npm --prefix apps/heiwa_app/desktop run tauri:build:app
  bundle="$target_root/release/bundle/macos/Heiwa.app"
  test -x "$bundle/Contents/MacOS/Heiwa"
  test -x "$bundle/Contents/Resources/resources/heiwa"
  rm -rf "$out/Heiwa.app"
  # Move rather than copy: the target directory may be shared with a checkout
  # at another revision, which must not find this bundle and install it as its own.
  mv "$bundle" "$out/Heiwa.app"
  resources="$out/Heiwa.app/Contents/Resources/resources"
  cp "$resources/heiwa" "$out/heiwa"
  if [[ -f "$resources/heiwa-apple-resources" ]]; then
    cp "$resources/heiwa-apple-resources" "$out/heiwa-apple-resources"
  fi
else
  echo "==> runtime"
  cargo build --locked --release -p heiwa-shell --bin heiwa --features heiwa-shell/lance
  cp "$target_root/release/heiwa" "$out/heiwa"
fi

test -s "$out/heiwa"
echo "==> built $(git rev-parse --short=12 HEAD) into $out"
