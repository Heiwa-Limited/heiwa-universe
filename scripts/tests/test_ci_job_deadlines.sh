#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture="$(mktemp "${TMPDIR:-/tmp}/heiwa-ci-deadlines.XXXXXX")"
log="$(mktemp "${TMPDIR:-/tmp}/heiwa-ci-deadlines-log.XXXXXX")"
trap 'rm -f "$fixture" "$log"' EXIT

cp "$repo_root/.github/workflows/ci.yml" "$fixture"
cat >>"$fixture" <<'YAML'

  _unbounded_job:
    name: Missing deadline fixture
    runs-on: ubuntu-latest
    steps:
      - run: true
YAML

if ruby "$repo_root/scripts/check_ci_job_deadlines.rb" "$fixture" >"$log" 2>&1; then
  echo "deadline checker accepted a valid underscore job ID without a deadline" >&2
  exit 1
fi
grep -Fq 'missing: _unbounded_job' "$log"

echo "CI job deadline checker tests passed."
