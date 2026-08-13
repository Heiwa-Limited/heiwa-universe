#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/package_release_sandbox.sh [--version VERSION] [--out DIR] [--skip-build]

Builds and packages the host-platform heiwa release artifact locally, without
uploading to GitHub or mutating the installed ~/.heiwa runtime.

Defaults:
  --version dev-<git-sha>
  --out /tmp/heiwa-release-sandbox
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version=""
out_root="/tmp/heiwa-release-sandbox"
skip_build=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      version="${2:-}"
      shift 2
      ;;
    --version=*)
      version="${1#--version=}"
      shift
      ;;
    --out)
      out_root="${2:-}"
      shift 2
      ;;
    --out=*)
      out_root="${1#--out=}"
      shift
      ;;
    --skip-build)
      skip_build=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$version" ]]; then
  git_sha="$(git -C "$repo_root" rev-parse --short HEAD)"
  version="dev-${git_sha}"
fi

case "$(uname -s):$(uname -m)" in
  Darwin:arm64)
    asset_name="macos-aarch64"
    target="aarch64-apple-darwin"
    archive_ext="tar.gz"
    binary_name="heiwa"
    ;;
  Linux:x86_64)
    asset_name="linux-x86_64"
    target="x86_64-unknown-linux-gnu"
    archive_ext="tar.gz"
    binary_name="heiwa"
    ;;
  MINGW*:x86_64|MSYS*:x86_64|CYGWIN*:x86_64)
    asset_name="windows-x86_64"
    target="x86_64-pc-windows-msvc"
    archive_ext="zip"
    binary_name="heiwa.exe"
    ;;
  *)
    echo "unsupported local release platform: $(uname -s):$(uname -m)" >&2
    exit 1
    ;;
esac

cd "$repo_root"

if [[ "$skip_build" == "0" ]]; then
  npm ci --ignore-scripts
  npm --prefix apps/heiwa_app/clients/cockpit run build
  cargo build --locked --release -p heiwa-shell --bin heiwa --target "$target"
fi

binary_path="target/${target}/release/${binary_name}"
cockpit_dist="apps/heiwa_app/clients/cockpit/dist"
if [[ ! -x "$binary_path" ]]; then
  echo "release binary missing or not executable: $binary_path" >&2
  exit 1
fi
if [[ ! -f "$cockpit_dist/index.html" ]] ||
  [[ -z "$(find "$cockpit_dist/assets" -type f -print -quit 2>/dev/null)" ]]; then
  echo "built cockpit assets are missing under $cockpit_dist" >&2
  exit 1
fi

run_id="$(date -u +%Y%m%dT%H%M%SZ)"
dist_dir="${out_root}/${run_id}"
release_root="${dist_dir}/heiwa-${version}-${asset_name}"
archive_path="${dist_dir}/heiwa-${version}-${asset_name}.${archive_ext}"
checksum_path="${dist_dir}/heiwa-${version}-checksums.txt"

mkdir -p "$release_root/cockpit"
cp "$binary_path" "$release_root/"
cp README.md CONTRIBUTING.md CODE_OF_CONDUCT.md LICENSE "$release_root/"
cp -R "$cockpit_dist"/. "$release_root/cockpit/"

case "$archive_ext" in
  tar.gz)
    tar -C "$dist_dir" -czf "$archive_path" "heiwa-${version}-${asset_name}"
    tar -tzf "$archive_path" | grep -q "heiwa-${version}-${asset_name}/${binary_name}$"
    tar -tzf "$archive_path" | grep -q "heiwa-${version}-${asset_name}/LICENSE$"
    tar -tzf "$archive_path" | grep -q "heiwa-${version}-${asset_name}/cockpit/index.html$"
    ;;
  zip)
    if ! command -v 7z >/dev/null 2>&1; then
      echo "7z is required for Windows zip packaging" >&2
      exit 1
    fi
    7z a "$archive_path" "$release_root" >/dev/null
    ;;
  *)
    echo "unsupported archive extension: $archive_ext" >&2
    exit 1
    ;;
esac

if command -v sha256sum >/dev/null 2>&1; then
  (cd "$dist_dir" && sha256sum "$(basename "$archive_path")" > "$(basename "$checksum_path")")
else
  (cd "$dist_dir" && shasum -a 256 "$(basename "$archive_path")" > "$(basename "$checksum_path")")
fi

"${release_root}/${binary_name}" app update --dry-run >/dev/null

cat <<EOF
heiwa release sandbox
  source: local checkout
  upload: false
  version: ${version}
  platform: ${asset_name}
  binary: ${binary_path}
  archive: ${archive_path}
  checksums: ${checksum_path}
  cockpit: ${release_root}/cockpit/index.html
  smoke: ${binary_name} app update --dry-run
EOF
