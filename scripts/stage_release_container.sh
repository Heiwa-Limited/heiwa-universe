#!/usr/bin/env bash
set -euo pipefail

tag="${1:-}"
output_dir="${2:-}"
repo="${HEIWA_RELEASE_REPO:-Heiwa-Limited/heiwa-universe}"

fail() {
  echo "stage release container: $*" >&2
  exit 1
}

[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "tag must look like v0.1.0"
[[ -n "$output_dir" && "$output_dir" != "/" && "$output_dir" != "." ]] ||
  fail "refusing unsafe output directory: $output_dir"
[[ ! -e "$output_dir" ]] || fail "output directory already exists: $output_dir"

for command_name in gh tar awk grep find install cp mkdir mktemp; do
  command -v "$command_name" >/dev/null 2>&1 || fail "missing required command: $command_name"
done

version="${tag#v}"
asset="heiwa-${version}-linux-x86_64.tar.gz"
checksums="heiwa-${version}-checksums.txt"
archive_root="heiwa-${version}-linux-x86_64"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/heiwa-container-stage.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

gh release download "$tag" \
  --repo "$repo" \
  --pattern "$asset" \
  --pattern "$checksums" \
  --dir "$tmp_dir"

expected="$(awk -v file="$asset" '$2 == file || $2 == "*" file { print $1 }' "$tmp_dir/$checksums")"
[[ "$expected" =~ ^[0-9a-fA-F]{64}$ ]] || fail "release checksum entry is missing or malformed"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp_dir/$asset" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$tmp_dir/$asset" | awk '{print $1}')"
else
  fail "missing SHA-256 tool"
fi
[[ "$actual" == "$expected" ]] || fail "checksum mismatch for $asset"

if ! tar -tzf "$tmp_dir/$asset" | while IFS= read -r archive_path; do
  case "$archive_path" in
    "$archive_root"|"$archive_root/"|"$archive_root/"*) ;;
    *) exit 1 ;;
  esac
  case "/$archive_path/" in
    *"/../"*|*"/./"*) exit 1 ;;
  esac
done; then
  fail "archive contains a path outside $archive_root"
fi
if ! tar -tvzf "$tmp_dir/$asset" | awk '{ type = substr($1, 1, 1) } type != "-" && type != "d" { exit 1 }'; then
  fail "archive contains links or unsupported entry types"
fi

tar -xzf "$tmp_dir/$asset" -C "$tmp_dir"
release_dir="$tmp_dir/$archive_root"
[[ -f "$release_dir/heiwa" ]] || fail "release archive does not contain heiwa"
[[ -f "$release_dir/cockpit/index.html" ]] || fail "release archive does not contain cockpit/index.html"
find "$release_dir/cockpit/assets" -type f -print | grep -q . || fail "release archive does not contain cockpit assets"

mkdir -p "$output_dir"
install -m 0755 "$release_dir/heiwa" "$output_dir/heiwa"
cp -R "$release_dir/cockpit" "$output_dir/cockpit"

echo "Staged $tag linux-x86_64 release bytes for container packaging."
echo "sha256=$actual"
