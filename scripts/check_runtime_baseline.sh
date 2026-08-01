#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

expect_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "Missing required baseline file: $path" >&2
    exit 1
  fi
}

expect_file "rust-toolchain.toml"
expect_file ".nvmrc"
expect_file ".node-version"
expect_file "package.json"
expect_file ".env.example"
expect_file ".github/workflows/ci.yml"
expect_file ".github/workflows/deploy.yml"
expect_file "apps/heiwa_core/Dockerfile"

required_rust_channel="1.95.0"
required_node_version="26.0.0"

rust_channel="$(awk -F'"' '/channel = / { print $2 }' rust-toolchain.toml)"
if [[ -z "$rust_channel" ]]; then
  echo "Could not parse Rust channel from rust-toolchain.toml" >&2
  exit 1
fi

if [[ "$rust_channel" != "$required_rust_channel" ]]; then
  echo "rust-toolchain.toml must pin Rust $required_rust_channel, found $rust_channel" >&2
  exit 1
fi

builder_line="$(grep -E '^FROM rust:[0-9]+\.[0-9]+-slim AS rust-builder$' apps/heiwa_core/Dockerfile || true)"
if [[ -z "$builder_line" ]]; then
  echo "Could not find rust-builder image pin in apps/heiwa_core/Dockerfile" >&2
  exit 1
fi

builder_version="${builder_line#FROM rust:}"
builder_version="${builder_version%-slim AS rust-builder}"
rust_major_minor="${rust_channel%.*}"

if [[ "$builder_version" != "$rust_major_minor" ]]; then
  echo "Rust baseline drift: rust-toolchain.toml=$rust_channel but Dockerfile builder pin=$builder_version" >&2
  exit 1
fi

nvmrc_version="$(tr -d '[:space:]' < .nvmrc)"
node_version_file="$(tr -d '[:space:]' < .node-version)"

if [[ "$nvmrc_version" != "$node_version_file" ]]; then
  echo ".nvmrc ($nvmrc_version) does not match .node-version ($node_version_file)" >&2
  exit 1
fi

if [[ "$nvmrc_version" != "$required_node_version" ]]; then
  echo ".nvmrc/.node-version must pin Node $required_node_version, found $nvmrc_version" >&2
  exit 1
fi

if ! grep -q '"typecheck"' package.json; then
  echo "package.json must define a root typecheck script" >&2
  exit 1
fi

if ! grep -q '"workspaces"' package.json; then
  echo "package.json must define npm workspaces" >&2
  exit 1
fi

required_node_major="${required_node_version%%.*}.x"
if ! grep -q "\"node\": \"${required_node_major}\"" package.json; then
  echo "package.json engines must pin Node ${required_node_major}" >&2
  exit 1
fi

for workflow in .github/workflows/ci.yml .github/workflows/deploy.yml; do
  if ! grep -Eq "actions/setup-node@v[0-9]+" "$workflow"; then
    echo "$workflow must set up Node explicitly" >&2
    exit 1
  fi

  if ! grep -Eq "node-version-file: ['\"]?\\.nvmrc['\"]?" "$workflow"; then
    echo "$workflow must source Node from .nvmrc" >&2
    exit 1
  fi
done

for workflow in .github/workflows/ci.yml .github/workflows/deploy.yml; do
  if ! grep -Eq "toolchain: ${required_rust_channel}" "$workflow"; then
    echo "$workflow must install Rust $required_rust_channel" >&2
    exit 1
  fi
done

for required_env in HEIWA_MACHINE_AUTH_TOKEN HEIWA_JWT_SIGNING_SECRET HEIWA_STATE_BACKEND; do
  if ! grep -q "^${required_env}=" .env.example; then
    echo ".env.example is missing canonical variable: $required_env" >&2
    exit 1
  fi
done

bash scripts/tests/test_audit_operator_machine.sh

echo "Runtime baseline pins and workflow wiring are present."
