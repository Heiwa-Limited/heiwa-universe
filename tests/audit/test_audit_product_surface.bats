#!/usr/bin/env bats

setup() {
    REPO_ROOT="$(git rev-parse --show-toplevel)"
    source "$REPO_ROOT/scripts/lib/parse_product_surface.sh"
}

@test "parse_product_surface extracts path -> class pairs from sample fixture" {
    local fixture="$REPO_ROOT/tests/audit/fixtures/sample_surface.md"
    run parse_product_surface "$fixture"
    [ "$status" -eq 0 ]
    [[ "$output" == *"crates/example product"* ]]
    [[ "$output" == *"apps/example_legacy legacy"* ]]
    [[ "$output" == *"packages/example_gen generated"* ]]
    [[ "$output" == *"docs/example_ref reference"* ]]
    [[ "$output" != *"product Active surface"* ]]
    [[ "$output" != *"legacy Old surface"* ]]
}

@test "parse_product_surface emits one path per line" {
    local fixture="$REPO_ROOT/tests/audit/fixtures/sample_surface.md"
    run parse_product_surface "$fixture"
    [ "$status" -eq 0 ]
    [ "$(echo "$output" | wc -l | tr -d ' ')" = "4" ]
}

@test "audit_product_surface.sh reports per-class LOC totals" {
    run "$REPO_ROOT/scripts/audit_product_surface.sh"
    [ "$status" -eq 0 ]
    [[ "$output" == *"product"* ]]
    [[ "$output" == *"legacy"* ]]
    [[ "$output" == *"LOC"* ]]
}

@test "audit_product_surface.sh exits non-zero when surface file is missing" {
    run env SURFACE_FILE=/nonexistent.md "$REPO_ROOT/scripts/audit_product_surface.sh"
    [ "$status" -ne 0 ]
}
