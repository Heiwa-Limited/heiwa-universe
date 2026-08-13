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
  /*) ;;
  *) fail "HEIWA_HOME must be an absolute path: $heiwa_home" ;;
esac

need curl
need grep
need awk
need tar
need install
need mv
need mktemp
need cp
need find
need ln
need mkdir

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
staged_cockpit=""
staged_link=""

cleanup() {
  rm -rf -- "$tmp_dir"
  if [ -n "$staged_path" ]; then
    rm -f -- "$staged_path"
  fi
  if [ -n "$staged_cockpit" ]; then
    rm -rf -- "$staged_cockpit"
  fi
  if [ -n "$staged_link" ]; then
    rm -f -- "$staged_link"
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
if ! tar -tvzf "$tmp_dir/$archive_name" | awk '
  { type = substr($1, 1, 1) }
  type != "-" && type != "d" { exit 1 }
'; then
  fail "archive contains links or unsupported entry types"
fi

tar -xzf "$tmp_dir/$archive_name" -C "$tmp_dir"
binary="$tmp_dir/$archive_root/heiwa"
[ -f "$binary" ] || fail "release archive does not contain the heiwa binary"
cockpit_source="$tmp_dir/$archive_root/cockpit"
[ -f "$cockpit_source/index.html" ] || fail "release archive does not contain cockpit/index.html"
find "$cockpit_source/assets" -type f -print | grep -q . ||
  fail "release archive does not contain cockpit assets"

bin_dir="$heiwa_home/bin"
app_dir="$heiwa_home/app"
mkdir -p "$bin_dir"
mkdir -p "$app_dir"
staged_path="$bin_dir/.heiwa.new.$$"
install -m 0755 "$binary" "$staged_path"

checksum_prefix="$(printf '%.12s' "$actual")"
cockpit_name="cockpit-$version-$checksum_prefix"
cockpit_target="$app_dir/$cockpit_name"
if [ ! -d "$cockpit_target" ]; then
  staged_cockpit="$app_dir/.cockpit.new.$$"
  cp -R "$cockpit_source" "$staged_cockpit"
  mv "$staged_cockpit" "$cockpit_target"
  staged_cockpit=""
fi
[ -f "$cockpit_target/index.html" ] || fail "installed cockpit target is incomplete"

cockpit_link="$app_dir/cockpit-current"
if [ -e "$cockpit_link" ] && [ ! -L "$cockpit_link" ]; then
  fail "$cockpit_link exists and is not a managed symlink"
fi
staged_link="$app_dir/.cockpit-current.$$"
ln -s "$cockpit_name" "$staged_link"
case "$os" in
  Darwin) mv -fh "$staged_link" "$cockpit_link" ;;
  Linux) mv -Tf "$staged_link" "$cockpit_link" ;;
esac
staged_link=""

mv -f "$staged_path" "$bin_dir/heiwa"
staged_path=""

echo "heiwa install: bootstrapping local runtime state"
"$bin_dir/heiwa" install

cat <<EOF
heiwa install: complete
  version: v$version
  binary: $bin_dir/heiwa
  cockpit: $cockpit_link
  archive: $archive_name
  sha256: $actual

Next:
  export PATH="$bin_dir:\$PATH"
  heiwa doctor
  heiwa app start --no-open
EOF
