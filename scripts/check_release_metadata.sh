#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail=0

require_file() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    echo "missing required file: $file" >&2
    fail=1
  fi
}

require_match() {
  local file="$1"
  local pattern="$2"
  local label="$3"

  require_file "$file"
  if [[ -f "$file" ]] && ! grep -Eq "$pattern" "$file"; then
    echo "release metadata check failed for $file: $label" >&2
    fail=1
  fi
}

require_match "LICENSE" "^ *Apache License$" "root Apache-2.0 license text is required"
require_match "Cargo.toml" 'license = "Apache-2\.0"' "workspace package license must be Apache-2.0"
require_match ".github/workflows/release.yml" 'cp README\.md CONTRIBUTING\.md CODE_OF_CONDUCT\.md LICENSE' "release archives must include LICENSE"

python_projects=(
  "pyproject.toml"
  "packages/heiwa_cli/pyproject.toml"
  "legacy/packages/heiwa_cognition/pyproject.toml"
  "packages/heiwa_sdk/pyproject.toml"
  "apps/heiwa_trading/pyproject.toml"
  "runtime/python/pyproject.toml"
)

for project in "${python_projects[@]}"; do
  require_match "$project" 'license = \{ text = "Apache-2\.0" \}' "Python project license must be Apache-2.0"
done

node_projects=(
  "package.json"
  "apps/heiwa_app/package.json"
  "apps/heiwa_app/clients/cockpit/package.json"
  "legacy/apps/heiwa_cli/package.json"
  "packages/heiwa_bindings/typescript/package.json"
)

for project in "${node_projects[@]}"; do
  require_match "$project" '"license": "Apache-2\.0"' "Node package license must be Apache-2.0"
done

if git ls-files 'Cargo.toml' '*Cargo.toml' '*pyproject.toml' '*package.json' \
  | grep -v '^legacy/packages/heiwa_skills/' \
  | xargs grep -n 'UNLICENSED' >/tmp/heiwa-release-metadata-unlicensed.$$ 2>/dev/null; then
  echo "UNLICENSED metadata remains in tracked releasable manifests:" >&2
  cat /tmp/heiwa-release-metadata-unlicensed.$$ >&2
  fail=1
fi
rm -f /tmp/heiwa-release-metadata-unlicensed.$$

if (( fail != 0 )); then
  exit 1
fi

echo "Release metadata check passed."
