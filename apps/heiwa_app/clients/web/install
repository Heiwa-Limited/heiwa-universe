#!/bin/sh
set -eu

default_repo_url="https://github.com/Strategizing/heiwa-universe.git"
repo_url="${HEIWA_REPO_URL:-$default_repo_url}"
repo_ref="${HEIWA_REF:-main}"
heiwa_home="${HEIWA_HOME:-$HOME/.heiwa}"
source_dir="${HEIWA_SOURCE_DIR:-$heiwa_home/src/heiwa-universe}"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "heiwa install: missing required command: $1" >&2
    echo "Install $2, then rerun this installer." >&2
    exit 1
  fi
}

git_auth() {
  if [ -n "${HEIWA_PRIVATE_TOKEN:-}" ]; then
    git -c "http.https://github.com/.extraheader=AUTHORIZATION: bearer $HEIWA_PRIVATE_TOKEN" "$@"
  else
    git "$@"
  fi
}

need git "Git"
need cargo "Rust from https://rustup.rs"

if [ "$repo_url" = "$default_repo_url" ] && [ -z "${HEIWA_PRIVATE_TOKEN:-}" ]; then
  cat >&2 <<'EOF'
heiwa install: the public release installer is not live yet.

This temporary source installer targets the private heiwa-universe repository.
Set HEIWA_PRIVATE_TOKEN to a GitHub token with repository read access, or wait
for the GitHub Release installer with checksums.
EOF
  exit 1
fi

mkdir -p "$heiwa_home/src"

if [ -d "$source_dir/.git" ]; then
  echo "heiwa install: updating source at $source_dir"
  git_auth -C "$source_dir" fetch --depth 1 origin "$repo_ref"
  git_auth -C "$source_dir" checkout --force FETCH_HEAD
else
  echo "heiwa install: cloning $repo_url#$repo_ref into $source_dir"
  rm -rf "$source_dir"
  git_auth clone --depth 1 --branch "$repo_ref" "$repo_url" "$source_dir"
fi

echo "heiwa install: building heiwa into $heiwa_home/bin/heiwa"
cargo install --path "$source_dir/apps/heiwa_shell" --root "$heiwa_home" --locked --force

echo "heiwa install: bootstrapping local runtime state"
"$heiwa_home/bin/heiwa" install

cat <<EOF
heiwa install: complete
  binary: $heiwa_home/bin/heiwa
  source: $source_dir

Next:
  export PATH="$heiwa_home/bin:\$PATH"
  heiwa doctor
  heiwa app start --no-open
EOF
