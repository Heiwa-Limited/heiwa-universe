#!/usr/bin/env bash
set -euo pipefail

mode="integration"
integration_branch="${HEIWA_INTEGRATION_BRANCH:-dev}"
production_ref="${HEIWA_PRODUCTION_REF:-refs/remotes/origin/main}"
integration_ref=""

usage() {
  cat >&2 <<'EOF'
Usage: scripts/check_branch_topology.sh [--mode integration|experimental|post-promotion]

Local-only branch topology gate. It never fetches. By default integration and
post-promotion compare the local integration branch `dev` with the cached
production ref `origin/main`; experimental compares cached `origin/dev`.

Modes:
  integration     require dev to be ahead of and not behind origin/main
  experimental    require a non-dev/main branch descended from cached origin/dev
  post-promotion  permit dev to be synchronized with, but never behind, main
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      [[ $# -ge 2 ]] || {
        usage
        exit 2
      }
      mode="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

case "$mode" in
  integration|experimental|post-promotion) ;;
  *)
    usage
    exit 2
    ;;
esac

if [[ -n "${HEIWA_INTEGRATION_REF:-}" ]]; then
  integration_ref="$HEIWA_INTEGRATION_REF"
elif [[ "$mode" == "experimental" ]]; then
  integration_ref="refs/remotes/origin/$integration_branch"
else
  integration_ref="refs/heads/$integration_branch"
fi

ref_label() {
  case "$1" in
    refs/remotes/*) printf '%s' "${1#refs/remotes/}" ;;
    refs/heads/*) printf '%s' "${1#refs/heads/}" ;;
    *) printf '%s' "$1" ;;
  esac
}

integration_label="$(ref_label "$integration_ref")"
production_label="$(ref_label "$production_ref")"

repo_root="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  printf 'FAIL: not inside a git repository\n' >&2
  exit 1
}
cd "$repo_root"

current_branch="$(git symbolic-ref --quiet --short HEAD 2>/dev/null || true)"
if [[ -z "$current_branch" ]]; then
  printf 'FAIL: checkout is detached\n' >&2
  exit 1
fi

if [[ "$mode" == "experimental" ]]; then
  if ! git show-ref --verify --quiet "$integration_ref"; then
    fetch_target="${integration_ref#refs/remotes/origin/}"
    printf 'FAIL: cached integration ref is missing: %s; run git fetch origin %s\n' \
      "$integration_label" "$fetch_target" >&2
    exit 1
  fi
else
  if ! git show-ref --verify --quiet "$integration_ref"; then
    printf 'FAIL: local integration branch is missing: %s\n' "$integration_branch" >&2
    exit 1
  fi
fi
if ! git show-ref --verify --quiet "$production_ref"; then
  printf 'FAIL: cached production ref is missing: %s\n' "$production_ref" >&2
  exit 1
fi

read -r behind ahead < <(
  git rev-list --left-right --count "$production_ref...$integration_ref"
)

if (( behind > 0 )); then
  printf 'FAIL: %s is behind %s by %s commit(s)\n' \
    "$integration_label" "$production_label" "$behind" >&2
  exit 1
fi

case "$mode" in
  experimental)
    if [[ "$current_branch" == "$integration_branch" || "$current_branch" == "main" ]]; then
      printf 'FAIL: experimental work must not run on %s\n' "$current_branch" >&2
      exit 1
    fi
    if (( ahead == 0 )); then
      printf 'OK: %s is synchronized with %s\n' "$integration_label" "$production_label"
    else
      printf 'OK: %s is %s commit(s) ahead of %s\n' \
        "$integration_label" "$ahead" "$production_label"
    fi
    if ! git merge-base --is-ancestor "$integration_ref" "$current_branch"; then
      printf 'FAIL: %s does not descend from %s\n' \
        "$current_branch" "$integration_label" >&2
      exit 1
    fi
    printf 'OK: %s descends from %s\n' "$current_branch" "$integration_label"
    ;;
  integration)
    if [[ "$current_branch" != "$integration_branch" ]]; then
      printf 'FAIL: checkout branch is %s; expected %s\n' \
        "$current_branch" "$integration_branch" >&2
      exit 1
    fi
    if (( ahead == 0 )); then
      printf 'FAIL: %s has no value-bearing commit ahead of origin/main\n' \
        "$integration_branch" >&2
      exit 1
    fi
    printf 'OK: %s is %s commit(s) ahead of origin/main\n' \
      "$integration_branch" "$ahead"
    ;;
  post-promotion)
    if [[ "$current_branch" != "$integration_branch" ]]; then
      printf 'FAIL: checkout branch is %s; expected %s\n' \
        "$current_branch" "$integration_branch" >&2
      exit 1
    fi
    if (( ahead == 0 )); then
      printf 'OK: %s is synchronized with origin/main\n' "$integration_branch"
    else
      printf 'OK: %s is %s commit(s) ahead of origin/main\n' \
        "$integration_branch" "$ahead"
    fi
    ;;
esac
