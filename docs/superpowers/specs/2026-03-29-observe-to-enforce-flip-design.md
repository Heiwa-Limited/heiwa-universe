# Design Spec: Observe to Enforce Flip

**Date:** 2026-03-29
**Status:** Approved
**Gate:** Required before Phase B multi-tenant rollout

## Problem
The `ExecutionHookManager` currently operates primarily in `observe` mode. It checks capability leases but only logs warnings (`[OBSERVE]`) when violations occur. This allows the system to function while the planner and router are tuned, but it provides no hard security boundary. To support multiple tenants and autonomous safety, the system must flip to a fail-closed `enforce` mode.

## Goal
Transition the Heiwa execution plane to fail-closed enforcement for tool and routing scopes, while providing a queryable readiness signal and a narrow, audited recovery path for the operator.

## Scope
- **Enforced:** `tool_scope_json`, `routing_lock_json`, Lease Presence, Identity Presence.
- **Observed (Non-blocking):** `filesystem_scope_json`, `network_scope_json`, `secret_scope_json`.
- **Primary Control:** `HEIWA_ROLLOUT_MODE` environment variable.

## High-Level Architecture

### 1. Rollout Modes
- `observe`: (Default) Log all violations with structured telemetry; allow all executions to proceed.
- `enforce`: Fail-closed. Deny execution if policy checks fail, except for the explicit Operator Bypass.

### 2. The Hook Logic (`before_tool_call`)
The hook must evaluate the following rules in order:

1.  **Identity Resolution:** Extract `owner_id` from the payload. If absent, decision is `DENY` (enforce) / `WOULD_DENY` (observe) with code `MISSING_OWNER_ID`.
2.  **State Lookup:** Query SpacetimeDB for the active lease.
    - If **Lookup Fails** (error/timeout): Decision is `DENY` (enforce) / `WOULD_DENY` (observe) with code `LOOKUP_FAILURE`.
    - If **Record Missing**:
        - If `owner_id == HEIWA_OPERATOR_OWNER_ID`: **ALLOW** with code `OPERATOR_BYPASS_MISSING_LEASE`.
        - Otherwise: **DENY** (enforce) / **WOULD_DENY** (observe) with code `MISSING_LEASE`.
3.  **Integrity Checks:** (Applies to ALL users if a lease exists)
    - **Field Presence:** `tool_scope_json` and `routing_lock_json` must exist. Fail with `MISSING_SCOPE_FIELD`.
    - **JSON Validity:** Fields must be valid JSON. Fail with `INVALID_SCOPE_JSON`.
    - **Tool Match:** Current tool must be in `tool_scope_json`. Fail with `TOOL_SCOPE_MISMATCH`.
    - **Routing Match:** `model`, `provider`, and `runtime` must match `routing_lock_json`. Fail with `ROUTING_LOCK_MISMATCH`.

### 3. Telemetry & Readiness Gating
To satisfy the production flip gate, the hook must emit structured telemetry to the `artifacts` table (`artifact_type="hook_audit"`).

#### `hook_audit` Artifact
- **Decision:** `DENIED`, `WOULD_DENY`, `OPERATOR_BYPASS`.
- **Decision Code:** Stable mode-neutral machine-readable code (e.g., `TOOL_SCOPE_MISMATCH`, `MISSING_LEASE`, `LOOKUP_FAILURE`).
- **Metadata:** `owner_id`, `is_operator_owner`, `rollout_mode`, `tool`, `lease_id`.

#### Production Flip Gate
The environment variable `HEIWA_ROLLOUT_MODE` may only be set to `enforce` in production when:
1.  Full automated test suite passes (including new enforce-mode deny/bypass tests in `apps/heiwa_hub/tests/test_execution_hooks.py`).
2.  STDB `hook_audit` artifacts show **48 hours of zero non-operator `WOULD_DENY` events** across ALL enforced rules (Identity, Lease Presence, Scope Fields, Tool Match, Routing Match).
3.  Manual bench run confirms:
    - `OPERATOR_BYPASS_MISSING_LEASE` works for emergency repairs.
    - Operator tasks with an existing mismatched lease are **DENIED** (preserving integrity).

## Implementation Plan (Summary)
1.  **SDK Update:** Refactor `ExecutionHookManager` to implement the Truth Table and structured telemetry.
2.  **STDB Update:** Ensure `register_artifact` is called pre-execution for denials/bypasses.
3.  **Enrichment:** Update `after_tool_call` to include `owner_id` and `hook_decision: ALLOWED` in the `execution_audit` record.
4.  **Verification:** Extend `apps/heiwa_hub/tests/test_execution_hooks.py` with the new enforcement and bypass scenarios.

## Security Considerations
- **No Privileged Defaults:** Missing `owner_id` is a denial, not a bypass.
- **Fail-Closed on Error:** Database lookup failures stop execution in enforce mode.
- **Auditability:** Every bypass path emits a distinct, queryable artifact.
