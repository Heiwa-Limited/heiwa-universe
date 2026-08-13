#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

failures=0
count=0
while IFS= read -r line; do
  count=$((count + 1))
  value="${line#*uses:}"
  value="${value#"${value%%[![:space:]]*}"}"
  reference="${value%%[[:space:]#]*}"
  if [[ "$reference" == ./* ]]; then
    continue
  fi
  if [[ ! "$reference" =~ ^[^@[:space:]]+@[0-9a-f]{40}$ ]]; then
    echo "mutable or malformed workflow action reference: $line" >&2
    failures=$((failures + 1))
  fi
done < <(
  grep -R -n -E --include='*.yml' --include='*.yaml' \
    '^[[:space:]]*-?[[:space:]]*uses:' .github/workflows
)

if (( count == 0 )); then
  echo "no workflow action references found" >&2
  exit 1
fi
if (( failures > 0 )); then
  exit 1
fi

echo "Workflow action pins are immutable ($count references)."
