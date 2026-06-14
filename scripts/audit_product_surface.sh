#!/usr/bin/env bash
# Reads PRODUCT_SURFACE.md, walks tracked files, and emits LOC totals by class.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
SURFACE_FILE="${SURFACE_FILE:-$REPO_ROOT/PRODUCT_SURFACE.md}"

if [[ ! -f "$SURFACE_FILE" ]]; then
  echo "audit_product_surface: surface file not found: $SURFACE_FILE" >&2
  exit 2
fi

# shellcheck source=scripts/lib/parse_product_surface.sh
source "$REPO_ROOT/scripts/lib/parse_product_surface.sh"

mapping_file="$(mktemp "${TMPDIR:-/tmp}/heiwa-product-surface-mapping.XXXXXX")"
counts_file="$(mktemp "${TMPDIR:-/tmp}/heiwa-product-surface-counts.XXXXXX")"
trap 'rm -f "$mapping_file" "$counts_file"' EXIT

parse_product_surface "$SURFACE_FILE" \
  | awk '{ print length($1), $0 }' \
  | sort -rn \
  | cut -d' ' -f2- \
  > "$mapping_file"

matches_prefix() {
  local file="$1"
  local path="$2"
  [[ "$file" == "$path" || "$file" == "$path/"* ]]
}

: > "$counts_file"
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
  done < "$mapping_file"

  if [[ -n "$matched_class" ]]; then
    printf '%s %s\n' "$matched_class" "$loc" >> "$counts_file"
  else
    printf 'unclassified %s\n' "$loc" >> "$counts_file"
  fi
done < <(cd "$REPO_ROOT" && git ls-files)

echo "=== Product Surface Audit ==="
echo "Surface file: $SURFACE_FILE"
echo ""
printf "%-20s %12s %12s\n" "CLASS" "FILES" "LOC"
printf "%-20s %12s %12s\n" "-----" "-----" "---"

awk '
  {
    files[$1] += 1
    loc[$1] += $2
  }
  END {
    class_count = split("product generated legacy reference archive vendored runtime-artifact", classes, " ")
    for (i = 1; i <= class_count; i++) {
      class = classes[i]
      printf "%-20s %12d %12d\n", class, files[class] + 0, loc[class] + 0
    }
    printf "%-20s %12d %12d\n", "unclassified", files["unclassified"] + 0, loc["unclassified"] + 0
  }
' "$counts_file"
