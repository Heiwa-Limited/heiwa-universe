#!/usr/bin/env bash
set -u -o pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIX=0
failures=0
warnings=0
fixes=0

if [ "${1:-}" = "--fix" ]; then
  FIX=1
fi

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

fixed() {
  fixes=$((fixes + 1))
  printf 'FIX %s\n' "$1"
}

mode_of() {
  stat -f '%Lp' "$1" 2>/dev/null || printf 'missing'
}

owner_of() {
  stat -f '%Su' "$1" 2>/dev/null || printf 'missing'
}

ensure_mode() {
  local path="$1"
  local wanted="$2"
  local label="$3"

  if [ ! -e "$path" ]; then
    warn "$label missing: ${path/#$HOME/~}"
    return 0
  fi

  if [ "$(owner_of "$path")" != "$(id -un)" ]; then
    warn "$label not owned by current user: ${path/#$HOME/~}"
    return 0
  fi

  local mode
  mode="$(mode_of "$path")"
  if [ "$mode" = "$wanted" ]; then
    pass "$label mode $wanted"
    return 0
  fi

  if [ "$FIX" -eq 1 ]; then
    chmod "$wanted" "$path" && fixed "$label chmod $wanted"
  else
    warn "$label mode is $mode, expected $wanted: ${path/#$HOME/~}"
  fi
}

check_contains() {
  local label="$1"
  local expected="$2"
  shift 2
  local output
  output="$({ "$@"; } 2>&1)"
  if printf '%s' "$output" | grep -qi "$expected"; then
    pass "$label"
  else
    warn "$label unknown/unexpected: ${output:-no output}"
  fi
}

section "macOS security posture"
if command -v fdesetup >/dev/null 2>&1; then
  check_contains "FileVault enabled" "FileVault is On" fdesetup status
else
  warn "fdesetup unavailable"
fi

if [ -x /usr/libexec/ApplicationFirewall/socketfilterfw ]; then
  check_contains "application firewall enabled" "enabled" /usr/libexec/ApplicationFirewall/socketfilterfw --getglobalstate
else
  warn "application firewall checker unavailable"
fi

if command -v spctl >/dev/null 2>&1; then
  check_contains "Gatekeeper assessments enabled" "assessments enabled" spctl --status
else
  warn "spctl unavailable"
fi

if command -v csrutil >/dev/null 2>&1; then
  check_contains "System Integrity Protection enabled" "enabled" csrutil status
else
  warn "csrutil unavailable"
fi

section "owner-local permissions"
ensure_mode "$HOME/.ssh" 700 "ssh directory"
ensure_mode "$HOME/.heiwa" 700 "heiwa runtime root"
ensure_mode "$HOME/.bash_profile" 600 "bash profile"

if [ -d "$HOME/.ssh" ]; then
  if [ "$FIX" -eq 1 ]; then
    find "$HOME/.ssh" -type f -name '*.pub' -exec chmod 644 {} +
    find "$HOME/.ssh" -type f ! -name '*.pub' -exec chmod 600 {} +
    fixed "ssh key file modes normalized"
  fi
  if find "$HOME/.ssh" -type f ! -name '*.pub' ! -perm 600 -print -quit | grep -q .; then
    warn "one or more non-public ssh files are not 600"
  else
    pass "non-public ssh files mode 600"
  fi
fi

for file in "$HOME/.netrc" "$HOME/.npmrc" "$HOME/.pypirc" "$HOME/.cargo/credentials" "$HOME/.cargo/credentials.toml"; do
  [ -e "$file" ] && ensure_mode "$file" 600 "credential file ${file/#$HOME/~}"
done

section "repo-local secret hygiene"
cd "$ROOT" || exit 1
tracked_env="$(git ls-files | grep -E '(^|/)\.env($|\.)' | grep -vE '(^|/)\.env\.example$|\.template$' || true)"
if [ -n "$tracked_env" ]; then
  fail "tracked non-example .env file(s): $(printf '%s' "$tracked_env" | tr '\n' ' ')"
else
  pass "no tracked non-example .env files"
fi

repo_env_count=0
while IFS= read -r -d '' file; do
  repo_env_count=$((repo_env_count + 1))
  if [ "$FIX" -eq 1 ]; then
    chmod 600 "$file" && fixed "repo env chmod 600 ${file#./}"
  fi
  if [ "$(mode_of "$file")" != "600" ]; then
    warn "repo env file is not 600: ${file#./}"
  fi
done < <(find . -maxdepth 3 -type f \( -name '.env' -o -name '.env.*' \) ! -name '.env.example' ! -name '*.example' ! -name '*.template' -print0)
pass "repo env file count checked: $repo_env_count"

section "security tool availability"
if command -v gitleaks >/dev/null 2>&1; then
  pass "gitleaks installed"
else
  warn "gitleaks missing"
fi
if command -v cargo-audit >/dev/null 2>&1 || command -v cargo >/dev/null 2>&1; then
  pass "cargo audit path available through repo gate"
else
  warn "cargo unavailable on PATH"
fi
if command -v uvx >/dev/null 2>&1; then
  pass "uvx installed"
else
  warn "uvx missing"
fi

section "machine security summary"
printf 'failures: %s\n' "$failures"
printf 'warnings: %s\n' "$warnings"
printf 'fixes: %s\n' "$fixes"

if [ "$failures" -ne 0 ]; then
  exit 1
fi
