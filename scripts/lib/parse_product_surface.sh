#!/usr/bin/env bash
# parse_product_surface SURFACE_FILE
# Emits "<path> <class>" lines from the PRODUCT_SURFACE markdown table.

parse_product_surface() {
  local surface_file="${1:-}"

  if [[ -z "$surface_file" ]]; then
    echo "parse_product_surface: missing surface file path" >&2
    return 2
  fi

  if [[ ! -f "$surface_file" ]]; then
    echo "parse_product_surface: file not found: $surface_file" >&2
    return 1
  fi

  awk -F'|' '
    /^\| `/ {
      path = $2
      class = $3
      gsub(/`/, "", path)
      gsub(/`/, "", class)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", path)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", class)
      if (path != "" && class != "" && class != "Class") {
        print path, class
      }
    }
  ' "$surface_file"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  parse_product_surface "${1:-PRODUCT_SURFACE.md}"
fi
