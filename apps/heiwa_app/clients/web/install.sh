#!/bin/sh
set -eu

version="${HEIWA_VERSION:-0.1.0}"
heiwa_home="${HEIWA_HOME:-$HOME/.heiwa}"
repo="Heiwa-Limited/heiwa-universe"

fail() {
  echo "heiwa install: $*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' ||
  fail "HEIWA_VERSION must be a stable semantic version such as 0.1.0"

case "$heiwa_home" in
  ""|"/"|".") fail "refusing unsafe HEIWA_HOME: $heiwa_home" ;;
esac

need curl
need grep
need awk
need tar
need install
need mv
need mktemp

os="$(uname -s)"
arch="$(uname -m)"
case "$os:$arch" in
  Darwin:arm64|Darwin:aarch64)
    asset="macos-aarch64"
    ;;
  Linux:x86_64|Linux:amd64)
    asset="linux-x86_64"
    ;;
  *)
    fail "unsupported platform $os/$arch; use the GitHub Release assets directly"
    ;;
esac

archive_name="heiwa-${version}-${asset}.tar.gz"
checksums_name="heiwa-${version}-checksums.txt"
release_base="https://github.com/${repo}/releases/download/v${version}"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/heiwa-install.XXXXXX")"
staged_path=""

cleanup() {
  rm -rf -- "$tmp_dir"
  if [ -n "$staged_path" ]; then
    rm -f -- "$staged_path"
  fi
}
trap cleanup EXIT HUP INT TERM

download() {
  curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error     --output "$2" "$1"
}

echo "heiwa install: downloading v$version for $asset"
download "$release_base/$archive_name" "$tmp_dir/$archive_name"
download "$release_base/$checksums_name" "$tmp_dir/$checksums_name"

expected="$(
  awk -v file="$archive_name" '$2 == file || $2 == "*" file { print $1 }'     "$tmp_dir/$checksums_name"
)"
case "$expected" in
  ""|*[!0-9a-fA-F]*) fail "release checksum entry is missing or malformed" ;;
esac
[ "${#expected}" -eq 64 ] || fail "release checksum must be SHA-256"

if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp_dir/$archive_name" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$tmp_dir/$archive_name" | awk '{ print $1 }')"
else
  fail "missing SHA-256 tool: install sha256sum or shasum"
fi
[ "$actual" = "$expected" ] || fail "checksum mismatch for $archive_name"

archive_root="heiwa-${version}-${asset}"
if ! tar -tzf "$tmp_dir/$archive_name" | while IFS= read -r path; do
  case "$path" in
    "$archive_root"|"$archive_root/"|"$archive_root/"*) ;;
    *) exit 1 ;;
  esac
  case "/$path/" in
    *"/../"*|*"/./"*) exit 1 ;;
  esac
done; then
  fail "archive contains a path outside $archive_root"
fi

tar -xzf "$tmp_dir/$archive_name" -C "$tmp_dir"
binary="$tmp_dir/$archive_root/heiwa"
[ -f "$binary" ] || fail "release archive does not contain the heiwa binary"

bin_dir="$heiwa_home/bin"
mkdir -p "$bin_dir"
staged_path="$bin_dir/.heiwa.new.$$"
install -m 0755 "$binary" "$staged_path"
mv -f "$staged_path" "$bin_dir/heiwa"
staged_path=""

echo "heiwa install: bootstrapping local runtime state"
"$bin_dir/heiwa" install

cat <<EOF
heiwa install: complete
  version: v$version
  binary: $bin_dir/heiwa
  archive: $archive_name
  sha256: $actual

Next:
  export PATH="$bin_dir:\$PATH"
  heiwa doctor
  heiwa app start --no-open
EOF
