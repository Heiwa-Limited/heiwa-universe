#!/usr/bin/env bash

set -euo pipefail

root="${1:-}"
if [[ -z "$root" ]]; then
  root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
fi

if ! command -v rg >/dev/null 2>&1; then
  echo "audit_active_path_doctrine: rg is required" >&2
  exit 2
fi

active_paths=(
  "AGENTS.md"
  "CLAUDE.md"
  "GEMINI.md"
  "HEIWA.md"
  "README.md"
  "docs"
  "ops"
  ".claude"
  ".gemini"
  "scripts"
)

scan_paths=()
for path in "${active_paths[@]}"; do
  if [[ -e "$root/$path" ]]; then
    scan_paths+=("$root/$path")
  fi
done

if [[ "${#scan_paths[@]}" -eq 0 ]]; then
  echo "audit_active_path_doctrine: no active doctrine paths found under $root" >&2
  exit 2
fi

hub_path="apps/heiwa""_hub"
cli_app_path="apps/heiwa""_cli"
limbs_path="apps/heiwa""_limbs"
cognition_path="packages/heiwa""_cognition"
ui_path="packages/heiwa""_ui"
skills_path="packages/heiwa""_skills"
legacy_hub_path="legacy/${hub_path}"
quarantined_paths=(
  "$hub_path"
  "$cli_app_path"
  "$limbs_path"
  "$cognition_path"
  "$ui_path"
  "$skills_path"
)

matches=()
for quarantined_path in "${quarantined_paths[@]}"; do
  pattern="(^|[^[:alnum:]_/.-])${quarantined_path}(/|$)"
  while IFS= read -r match; do
    [[ -z "$match" ]] && continue
    matches+=("$match")
  done < <(
    rg -n --no-heading "$pattern" "${scan_paths[@]}" \
      --glob '!docs/superpowers/**' \
      --glob '!docs/design/**' \
      --glob '!docs/audit/**' \
      --glob '!docs/enterprise/**' \
      --glob '!docs/references/**' \
      --glob '!scripts/audit_active_path_doctrine.sh' \
      --glob '!**/.git/**' || true
  )
done

if [[ "${#matches[@]}" -gt 0 ]]; then
  echo "Active doctrine names quarantined paths as live:"
  printf '%s\n' "${matches[@]}"
  echo
  echo "Use current product paths or mark references with the matching legacy/ prefix, for example ${legacy_hub_path}/."
  exit 1
fi

echo "active-path-doctrine: ok"
