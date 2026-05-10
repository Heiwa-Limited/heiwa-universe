#!/usr/bin/env bats

# Connector truth gate audit. Runs scripts/validate_connector_manifests.py
# against the real connectors/ tree and against fixture manifests that
# exercise each fail-closed rule.

setup() {
    REPO_ROOT="$(git rev-parse --show-toplevel)"
    VALIDATOR="$REPO_ROOT/scripts/validate_connector_manifests.py"
    FIXTURE_BASE="$BATS_TEST_TMPDIR/connector_fixtures"
    SCHEMA_SRC="$REPO_ROOT/connectors/schema/connector.schema.json"
}

# Build an isolated repo-shaped sandbox containing only the schema and the
# manifest under test, then run the validator against it.
run_validator_in_sandbox() {
    local manifest_path="$1"
    local doc_claim="${2:-}"
    local sandbox
    local validator_status
    sandbox="$(mktemp -d "$BATS_TEST_TMPDIR/connector_sandbox.XXXXXX")"
    mkdir -p "$sandbox/connectors/schema" "$sandbox/scripts"
    cp "$SCHEMA_SRC" "$sandbox/connectors/schema/connector.schema.json"
    cp "$VALIDATOR" "$sandbox/scripts/validate_connector_manifests.py"
    cp "$manifest_path" "$sandbox/connectors/$(basename "$manifest_path")"
    if [[ -n "$doc_claim" ]]; then
        mkdir -p "$sandbox/docs"
        printf '%s\n' "$doc_claim" > "$sandbox/docs/product-contract.md"
    fi
    (
        cd "$sandbox"
        git init -q .
        git add -A
        git -c user.email=t@t -c user.name=t commit -q -m init >/dev/null
        python3 scripts/validate_connector_manifests.py
    )
    validator_status=$?
    rm -rf "$sandbox"
    return $validator_status
}

@test "validator passes against the real connectors/ tree" {
    run python3 "$VALIDATOR"
    [ "$status" -eq 0 ]
    [[ "$output" == *'"ok": true'* ]]
    [[ "$output" == *'connectors/github.connector.json'* ]]
}

@test "validator output is valid JSON" {
    run python3 "$VALIDATOR"
    [ "$status" -eq 0 ]
    echo "$output" | python3 -c 'import json,sys; json.loads(sys.stdin.read())'
}

@test "real GitHub manifest declares revocation_path and read-only capability" {
    run python3 -c '
import json, pathlib
m = json.loads(pathlib.Path("connectors/github.connector.json").read_text())
assert m["support_level"] == "official_api"
assert m["revocation_path"]
assert m["receipt_required"] is True
ids = [c["id"] for c in m["capabilities"]]
assert ids == ["github.repo.metadata.read"]
side_effects = {c.get("side_effect", "read") for c in m["capabilities"]}
assert side_effects == {"read"}
'
    [ "$status" -eq 0 ]
}

@test "fail-closed: official_api manifest without revocation_path" {
    mkdir -p "$FIXTURE_BASE"
    local f="$FIXTURE_BASE/no_revocation.connector.json"
    cat > "$f" <<'JSON'
{
  "id": "demo",
  "name": "Demo",
  "support_level": "official_api",
  "auth": ["api_key"],
  "receipt_required": true,
  "capabilities": [
    {
      "id": "demo.thing.read",
      "description": "read a thing",
      "risk_class": "silent",
      "receipt_required": true,
      "support_status": "target",
      "permission_notes": "stub"
    }
  ]
}
JSON
    run run_validator_in_sandbox "$f"
    [ "$status" -ne 0 ]
    [[ "$output" == *'"missing_revocation"'* ]]
}

@test "fail-closed: write capability without receipt_required" {
    mkdir -p "$FIXTURE_BASE"
    local f="$FIXTURE_BASE/write_no_receipt.connector.json"
    cat > "$f" <<'JSON'
{
  "id": "demo",
  "name": "Demo",
  "support_level": "official_api",
  "auth": ["api_key"],
  "revocation_path": "revoke at demo settings",
  "receipt_required": true,
  "capabilities": [
    {
      "id": "demo.thing.write",
      "description": "write a thing",
      "risk_class": "approve",
      "receipt_required": false,
      "support_status": "target",
      "side_effect": "write",
      "permission_notes": "stub"
    }
  ]
}
JSON
    run run_validator_in_sandbox "$f"
    [ "$status" -ne 0 ]
    [[ "$output" == *'"missing_receipt_for_write"'* ]]
}

@test "fail-closed: target connector cannot claim live capability" {
    mkdir -p "$FIXTURE_BASE"
    local f="$FIXTURE_BASE/target_live.connector.json"
    cat > "$f" <<'JSON'
{
  "id": "demo",
  "name": "Demo",
  "support_level": "target",
  "auth": ["api_key"],
  "receipt_required": true,
  "capabilities": [
    {
      "id": "demo.thing.read",
      "description": "read a thing",
      "risk_class": "silent",
      "receipt_required": true,
      "support_status": "live",
      "permission_notes": "stub"
    }
  ]
}
JSON
    run run_validator_in_sandbox "$f"
    [ "$status" -ne 0 ]
    [[ "$output" == *'"target_with_live_capability"'* ]]
}

@test "fail-closed: unsupported connector cannot claim live capability" {
    mkdir -p "$FIXTURE_BASE"
    local f="$FIXTURE_BASE/unsupported_live.connector.json"
    cat > "$f" <<'JSON'
{
  "id": "demo",
  "name": "Demo",
  "support_level": "unsupported",
  "auth": ["api_key"],
  "receipt_required": true,
  "capabilities": [
    {
      "id": "demo.thing.read",
      "description": "read a thing",
      "risk_class": "silent",
      "receipt_required": true,
      "support_status": "live",
      "permission_notes": "stub"
    }
  ]
}
JSON
    run run_validator_in_sandbox "$f"
    [ "$status" -ne 0 ]
    [[ "$output" == *'"unsupported_with_live_capability"'* ]]
}

@test "fail-closed: invalid support_level enum" {
    mkdir -p "$FIXTURE_BASE"
    local f="$FIXTURE_BASE/bad_enum.connector.json"
    cat > "$f" <<'JSON'
{
  "id": "demo",
  "name": "Demo",
  "support_level": "magic",
  "auth": ["api_key"],
  "receipt_required": true,
  "capabilities": [
    {
      "id": "demo.thing.read",
      "description": "read a thing",
      "risk_class": "silent",
      "receipt_required": true,
      "support_status": "target",
      "permission_notes": "stub"
    }
  ]
}
JSON
    run run_validator_in_sandbox "$f"
    [ "$status" -ne 0 ]
    [[ "$output" == *'"invalid_enum"'* ]]
}

@test "fail-closed: capability missing risk_class" {
    mkdir -p "$FIXTURE_BASE"
    local f="$FIXTURE_BASE/no_risk_class.connector.json"
    cat > "$f" <<'JSON'
{
  "id": "demo",
  "name": "Demo",
  "support_level": "official_api",
  "auth": ["api_key"],
  "revocation_path": "revoke at demo settings",
  "receipt_required": true,
  "capabilities": [
    {
      "id": "demo.thing.read",
      "description": "read a thing",
      "receipt_required": true,
      "support_status": "target",
      "permission_notes": "stub"
    }
  ]
}
JSON
    run run_validator_in_sandbox "$f"
    [ "$status" -ne 0 ]
    [[ "$output" == *'"missing_risk_class"'* || "$output" == *'"missing_field"'* ]]
}

@test "fail-closed: duplicate capability ids" {
    mkdir -p "$FIXTURE_BASE"
    local f="$FIXTURE_BASE/duplicate_capability.connector.json"
    cat > "$f" <<'JSON'
{
  "id": "demo",
  "name": "Demo",
  "support_level": "official_api",
  "auth": ["api_key"],
  "revocation_path": "revoke at demo settings",
  "receipt_required": true,
  "capabilities": [
    {
      "id": "demo.thing.read",
      "description": "read a thing",
      "risk_class": "silent",
      "receipt_required": true,
      "support_status": "target",
      "permission_notes": "stub"
    },
    {
      "id": "demo.thing.read",
      "description": "read a thing again",
      "risk_class": "silent",
      "receipt_required": true,
      "support_status": "target",
      "permission_notes": "stub"
    }
  ]
}
JSON
    run run_validator_in_sandbox "$f"
    [ "$status" -ne 0 ]
    [[ "$output" == *'"duplicate_capability"'* ]]
}

@test "fail-closed: unknown manifest field" {
    mkdir -p "$FIXTURE_BASE"
    local f="$FIXTURE_BASE/unknown_field.connector.json"
    cat > "$f" <<'JSON'
{
  "id": "demo",
  "name": "Demo",
  "support_level": "official_api",
  "auth": ["api_key"],
  "revocation_path": "revoke at demo settings",
  "receipt_required": true,
  "magical_extra": true,
  "capabilities": [
    {
      "id": "demo.thing.read",
      "description": "read a thing",
      "risk_class": "silent",
      "receipt_required": true,
      "support_status": "target",
      "permission_notes": "stub"
    }
  ]
}
JSON
    run run_validator_in_sandbox "$f"
    [ "$status" -ne 0 ]
    [[ "$output" == *'"unknown_field"'* ]]
}

@test "fail-closed: unbacked docs connector claim" {
    mkdir -p "$FIXTURE_BASE"
    local f="$FIXTURE_BASE/demo.connector.json"
    cat > "$f" <<'JSON'
{
  "id": "demo",
  "name": "Demo",
  "support_level": "official_api",
  "auth": ["api_key"],
  "revocation_path": "revoke at demo settings",
  "receipt_required": true,
  "capabilities": [
    {
      "id": "demo.thing.read",
      "description": "read a thing",
      "risk_class": "silent",
      "receipt_required": true,
      "support_status": "target",
      "permission_notes": "stub"
    }
  ]
}
JSON
    run run_validator_in_sandbox "$f" "Slack connector is live."
    [ "$status" -ne 0 ]
    [[ "$output" == *'"unbacked_product_claim"'* ]]
}

@test "fail-closed: docs cannot claim live connector without live capability" {
    mkdir -p "$FIXTURE_BASE"
    local f="$FIXTURE_BASE/demo.connector.json"
    cat > "$f" <<'JSON'
{
  "id": "demo",
  "name": "Demo",
  "support_level": "official_api",
  "auth": ["api_key"],
  "revocation_path": "revoke at demo settings",
  "receipt_required": true,
  "claim_aliases": ["Demo"],
  "capabilities": [
    {
      "id": "demo.thing.read",
      "description": "read a thing",
      "risk_class": "silent",
      "receipt_required": true,
      "support_status": "target",
      "permission_notes": "stub"
    }
  ]
}
JSON
    run run_validator_in_sandbox "$f" "Demo connector is live."
    [ "$status" -ne 0 ]
    [[ "$output" == *'"overstated_product_claim"'* ]]
}
