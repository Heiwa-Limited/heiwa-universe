#!/usr/bin/env bash
set -u -o pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
PYTHON_AUDIT_PYTHON="${PYTHON_AUDIT_PYTHON:-3.14}"

failures=0
warnings=0

section() {
  printf '\n== %s ==\n' "$1"
}

pass() {
  printf 'PASS %s\n' "$1"
}

warn() {
  warnings=$((warnings + 1))
  printf 'WARN %s\n' "$1"
}

fail() {
  failures=$((failures + 1))
  printf 'FAIL %s\n' "$1"
}

run_required() {
  local label="$1"
  shift
  section "$label"
  if "$@"; then
    pass "$label"
  else
    local status=$?
    fail "$label (exit $status)"
  fi
}

run_optional() {
  local label="$1"
  shift
  section "$label"
  if "$@"; then
    pass "$label"
  else
    local status=$?
    warn "$label skipped/failed (exit $status)"
  fi
}

require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    fail "missing command: $cmd"
    return 1
  fi
}

audit_python_project() {
  local label="$1"
  local project_dir="$2"
  local output_file="$TMPDIR/${label//[^A-Za-z0-9_.-]/_}.requirements.txt"

  require_command uv || return 1

  (
    cd "$project_dir" &&
      uv export \
        --frozen \
        --all-extras \
        --format requirements.txt \
        --no-hashes \
        --no-emit-project \
        --no-emit-local \
        --output-file "$output_file" >/dev/null &&
      uv tool run --python "$PYTHON_AUDIT_PYTHON" pip-audit \
        --cache-dir "$TMPDIR/pip-audit-cache-$label" \
        -r "$output_file"
  )
}

cd "$ROOT" || exit 1

section "security gate metadata"
printf 'root: %s\n' "$ROOT"
printf 'python-audit-python: %s\n' "$PYTHON_AUDIT_PYTHON"
printf 'git: %s\n' "$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
printf 'dirty: %s\n' "$(test -z "$(git status --porcelain 2>/dev/null)" && echo false || echo true)"

run_required "cargo audit" cargo audit
run_required "root npm prod audit" npm audit --omit=dev
run_required "root TypeScript typecheck" npm run typecheck --silent
run_required "desktop TypeScript typecheck" bash -c 'cd apps/heiwa_app/desktop && npm run typecheck --silent'
run_required "Deno herd/prototype typecheck" deno check scripts/herd.ts prototypes/heiwa-desk/main.ts prototypes/heiwa-desk/herdr.ts
run_required "root Python dependency audit" audit_python_project root "$ROOT"
run_required "runtime Python dependency audit" audit_python_project runtime-python "$ROOT/runtime/python"
run_required "product surface audit" bash scripts/audit_product_surface.sh

if command -v gitleaks >/dev/null 2>&1; then
  run_optional "gitleaks secret scan (redacted, non-blocking until repo baseline exists)" \
    gitleaks detect --no-banner --redact --source "$ROOT"
else
  section "gitleaks secret scan"
  warn "gitleaks not installed; install it to make secret scanning mandatory"
fi

section "security gate summary"
printf 'failures: %s\n' "$failures"
printf 'warnings: %s\n' "$warnings"

if [ "$failures" -ne 0 ]; then
  exit 1
fi
