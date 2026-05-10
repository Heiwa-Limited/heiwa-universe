#!/usr/bin/env bats

setup() {
    REPO_ROOT="$(git rev-parse --show-toplevel)"
}

@test "active path doctrine passes for repo" {
    run "$REPO_ROOT/scripts/audit_active_path_doctrine.sh"
    [ "$status" -eq 0 ]
    [[ "$output" == *"active-path-doctrine: ok"* ]]
}

@test "active path doctrine fails on live Hub path" {
    sandbox="$BATS_TEST_TMPDIR/repo"
    mkdir -p "$sandbox/docs"
    printf 'Active STDB path: `apps/heiwa_hub/spacetimedb/`\n' > "$sandbox/HEIWA.md"

    run "$REPO_ROOT/scripts/audit_active_path_doctrine.sh" "$sandbox"
    [ "$status" -eq 1 ]
    [[ "$output" == *"apps/heiwa_hub/spacetimedb"* ]]
}

@test "active path doctrine allows explicit legacy Hub path" {
    sandbox="$BATS_TEST_TMPDIR/repo"
    mkdir -p "$sandbox/docs"
    printf 'Legacy STDB path: `legacy/apps/heiwa_hub/spacetimedb/`\n' > "$sandbox/HEIWA.md"

    run "$REPO_ROOT/scripts/audit_active_path_doctrine.sh" "$sandbox"
    [ "$status" -eq 0 ]
}

@test "active path doctrine fails on other quarantined live paths" {
    sandbox="$BATS_TEST_TMPDIR/repo"
    mkdir -p "$sandbox/ops"
    printf 'Route docs: `packages/heiwa_cognition/heiwa_cognition/router.py`\n' > "$sandbox/ops/HEIWA.md"

    run "$REPO_ROOT/scripts/audit_active_path_doctrine.sh" "$sandbox"
    [ "$status" -eq 1 ]
    [[ "$output" == *"packages/heiwa_cognition"* ]]
}
