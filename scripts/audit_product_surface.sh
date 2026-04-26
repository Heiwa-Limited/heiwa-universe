#!/usr/bin/env bash
# Reads PRODUCT_SURFACE.md, walks tracked files, and emits LOC totals by class.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
SURFACE_FILE="${SURFACE_FILE:-$REPO_ROOT/PRODUCT_SURFACE.md}"

if [[ ! -f "$SURFACE_FILE" ]]; then
  echo "audit_product_surface: surface file not found: $SURFACE_FILE" >&2
  exit 2
fi

# shellcheck source=lib/parse_product_surface.sh
source "$REPO_ROOT/scripts/lib/parse_product_surface.sh"

mapping="$(
  parse_product_surface "$SURFACE_FILE" \
    | awk '{ print length($1), $0 }' \
    | sort -rn \
    | cut -d' ' -f2-
)"

declare -A class_loc=()
declare -A class_files=()
declare -i unclassified_loc=0
declare -i unclassified_files=0

matches_prefix() {
  local file="$1"
  local path="$2"
  [[ "$file" == "$path" || "$file" == "$path/"* ]]
}

while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  [[ ! -f "$REPO_ROOT/$file" ]] && continue

  loc="$(wc -l < "$REPO_ROOT/$file" 2>/dev/null || printf '0')"
  loc="${loc//[[:space:]]/}"

  matched_class=""
  while IFS=' ' read -r path class; do
    [[ -z "${path:-}" || -z "${class:-}" ]] && continue
    if matches_prefix "$file" "$path"; then
      matched_class="$class"
      break
    fi
  done <<< "$mapping"

  if [[ -n "$matched_class" ]]; then
    class_loc[$matched_class]=$((${class_loc[$matched_class]:-0} + loc))
    class_files[$matched_class]=$((${class_files[$matched_class]:-0} + 1))
  else
    unclassified_loc=$((unclassified_loc + loc))
    unclassified_files=$((unclassified_files + 1))
  fi
done < <(cd "$REPO_ROOT" && git ls-files)

echo "=== Product Surface Audit ==="
echo "Surface file: $SURFACE_FILE"
echo ""
printf "%-20s %12s %12s\n" "CLASS" "FILES" "LOC"
printf "%-20s %12s %12s\n" "-----" "-----" "---"
for class in product generated legacy reference archive vendored runtime-artifact; do
  printf "%-20s %12s %12s\n" "$class" "${class_files[$class]:-0}" "${class_loc[$class]:-0}"
done
printf "%-20s %12s %12s\n" "unclassified" "$unclassified_files" "$unclassified_loc"
