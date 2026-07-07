# Heiwa Foundation Phase 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Heiwa's execution spine first-class by formalizing worker/session/lease state in STDB, adding replayable run receipts, reducing Python to a bounded bridge, and turning the new TypeScript workspace into the real shell foundation.

**Architecture:** Rust remains the only execution authority and STDB remains the only canonical state truth. The work starts by replacing overloaded node/lease/task primitives with explicit worker-session and run-receipt records, then updates `heiwa-core` to use those records, narrows Python to legacy-bridge behavior only, and finally tightens the TypeScript/web surface so the product shell is built on the same runtime contract.

**Tech Stack:** Rust 1.93.1, Axum, Tokio, SpacetimeDB, Python 3.14 bridge code, TypeScript 5.9.3, Node 24.14.1, npm workspaces, GitHub Actions, Railway.

**Path Convention:** All file paths in this plan are repo-relative and all commands are assumed to run from the repository root.

**Verified Baselines:** Python `3.14` is already the live baseline in `pyproject.toml` and `.github/workflows/deploy.yml`. This plan hardens around that current truth rather than introducing a new Python lane.

**Canonical Initial Status Values:** Use these exact names unless a task explicitly revises them:

- `worker_sessions`: `active | expired | closed`
- `leases`: `issued | acked | running | completed | failed | expired | revoked`
- `dispatch_acks`: `accepted | rejected`

---

## File Map

### State authority

- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `packages/heiwa_bindings/rust/Cargo.toml` if bindings regeneration changes package metadata
- Modify: `packages/heiwa_bindings/typescript/index.ts` only if regenerated bindings change exports

### Runtime authority

- Modify: `apps/heiwa_core/src/runtime/gateway.rs`
- Modify: `apps/heiwa_core/src/runtime/state.rs`
- Modify: `apps/heiwa_core/src/stdb/mod.rs`
- Modify: `apps/heiwa_core/src/runtime/mod.rs` only if routing changes

### Python bridge

- Modify: `apps/heiwa_cli/scripts/agents/worker_manager.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/adapters/acp.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/config.py`

### Web / TypeScript surface

- Modify: `apps/heiwa_web/clients/web/assets/*.js`
- Modify: `apps/heiwa_web/tsconfig.json`
- Modify: `package.json`
- Modify: `packages/heiwa_bindings/typescript/package.json`

### Tests

- Modify: `apps/heiwa_core/tests/worker_mesh.rs`
- Create: `apps/heiwa_core/tests/run_receipts.rs`
- Create: `apps/heiwa_core/tests/legacy_bridge.rs`
- Create or modify: `apps/heiwa_hub/tests/` reducer-facing tests if current STDB harness already exists there

### Docs / ops

- Modify: `docs/standards/runtime-baseline.md`
- Modify: `README.md`
- Modify: `.github/workflows/deploy.yml`

## Deprecated Table Cutover Gate

Do **not** demote or remove legacy state surfaces such as `nodes`, `capability_leases`, or `task_dispatches` until all of the following are true:

- all `worker_mesh`, `run_receipts`, and `legacy_bridge` tests pass
- `heiwa-core` reads canonical session/lease state only for core worker execution flows
- the legacy bridge translation path is verified against the canonical v1 lifecycle
- no core execution path depends on deprecated tables as the source of session/lease truth

Only after that gate is met should a follow-up plan remove or demote deprecated tables.

## Task 1: Formalize Worker Sessions and Leases in STDB

**Files:**

- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Test: existing STDB reducer tests or a new reducer-focused test file under `apps/heiwa_hub/tests/`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write the failing reducer tests for explicit worker-session records**

Add tests that expect STDB to support:

- `worker_sessions`
- `leases`
- `dispatch_acks`

Required behaviors:

- register a worker session without a lease
- issue one task-bound lease to a session
- reject lease lookup when session/task mismatch occurs

- [ ] **Step 2: Run the reducer tests to verify they fail**

Run the narrowest available STDB/reducer test command. If there is no existing isolated reducer test harness, add one and run that harness first.

Expected: failures because `worker_sessions`, `dispatch_acks`, or equivalent reducers/tables do not exist yet.

- [ ] **Step 3: Add minimal STDB tables and reducers**

In `apps/heiwa_hub/spacetimedb/src/lib.rs`, add explicit canonical records:

- `worker_sessions`
- `leases`
- `dispatch_acks`

Do not remove existing `nodes`, `capability_leases`, or `task_dispatches` yet. First add the new authoritative structures, then bridge the runtime to them.

- [ ] **Step 4: Add reducers for lifecycle changes**

Add the minimum reducers needed for:

- session create/update/expire
- lease issue/revoke/expire
- dispatch ack accept/reject

Use explicit status strings or enums consistently. Do not infer session state from node heartbeats anymore.

- [ ] **Step 5: Run the reducer tests to verify they pass**

Run the same reducer-focused command from Step 2.

Expected: passing tests proving explicit session and lease state exists.

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs apps/heiwa_hub/tests
git commit -m "feat: add explicit worker session and lease records"
```

## Task 2: Add Run Receipts and Replayable Failure Records

**Files:**

- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `apps/heiwa_core/src/stdb/mod.rs`
- Test: `apps/heiwa_core/tests/run_receipts.rs`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write the failing runtime test for run receipts**

Add a failing test in `apps/heiwa_core/tests/run_receipts.rs` that expects:

- every successful result creates a receipt-like persisted run record
- every error creates a failure record with a structured code
- every run links to at least one artifact location or failure log artifact

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p heiwa-core --test run_receipts -- --nocapture
```

Expected: FAIL because receipt/failure persistence is incomplete or inferred indirectly.

- [ ] **Step 3: Add explicit receipt/failure state to STDB**

In `apps/heiwa_hub/spacetimedb/src/lib.rs`, add the minimum explicit structures necessary for:

- run receipts
- failure classification
- replay metadata

Keep this minimal and evidence-first. Do not build a full replay engine yet.

- [ ] **Step 4: Update Rust STDB access layer**

In `apps/heiwa_core/src/stdb/mod.rs`, expose narrow helpers for:

- writing run receipts
- writing structured failures
- attaching artifact metadata

- [ ] **Step 5: Make the test pass with minimal runtime changes**

Update runtime persistence to use the new helpers. Do not refactor the gateway broadly in this task.

- [ ] **Step 6: Re-run the receipt test and core test suite**

Run:

```bash
cargo test -p heiwa-core --test run_receipts -- --nocapture
cargo test -p heiwa-core --quiet
```

Expected: both pass.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs apps/heiwa_core/src/stdb/mod.rs apps/heiwa_core/tests/run_receipts.rs
git commit -m "feat: persist run receipts and structured failures"
```

## Task 3: Move `heiwa-core` Runtime to the Explicit STDB Session/Lease Model

**Files:**

- Modify: `apps/heiwa_core/src/runtime/state.rs`
- Modify: `apps/heiwa_core/src/runtime/gateway.rs`
- Modify: `apps/heiwa_core/tests/worker_mesh.rs`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write a failing runtime test for canonical dispatch against persisted session/lease state**

Extend `apps/heiwa_core/tests/worker_mesh.rs` so it expects:

- `REGISTER` creates a persisted session
- `DISPATCH` creates a persisted lease
- `DISPATCH_ACK` updates explicit dispatch ack state
- `RESULT` or `ERROR` closes the lease and writes run receipt/failure state

- [ ] **Step 2: Run the worker mesh test to verify it fails**

Run:

```bash
cargo test -p heiwa-core --test worker_mesh -- --nocapture
```

Expected: FAIL because runtime still depends partly on in-memory lease/session truth.

- [ ] **Step 3: Narrow in-memory registry responsibilities**

In `apps/heiwa_core/src/runtime/state.rs`, keep the in-memory registry only for:

- connected websocket senders
- transient scheduling choices
- short-lived liveness cache

Move canonical session/lease truth to STDB-backed reads/writes.

- [ ] **Step 4: Update gateway lifecycle**

In `apps/heiwa_core/src/runtime/gateway.rs`, change:

- `REGISTER`
- `DISPATCH`
- `DISPATCH_ACK`
- `RESULT`
- `ERROR`

to read/write the explicit STDB session/lease/dispatch-ack/receipt state instead of treating the Rust registry as canonical truth.

- [ ] **Step 5: Re-run worker mesh tests**

Run:

```bash
cargo test -p heiwa-core --test worker_mesh -- --nocapture
cargo test -p heiwa-core --quiet
```

Expected: pass.

- [ ] **Step 6: Verify no deprecated table reads remain in core truth paths**

Run:

```bash
rg -n "capability_leases|task_dispatches|nodes" apps/heiwa_core/src/runtime apps/heiwa_core/src/stdb
```

Expected: no canonical session/lease truth reads remain against deprecated tables. Any remaining matches must be compatibility writes, comments, or clearly marked transitional code rather than read-side authority.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_core/src/runtime/state.rs apps/heiwa_core/src/runtime/gateway.rs apps/heiwa_core/tests/worker_mesh.rs
git commit -m "feat: move runtime session and lease truth into STDB"
```

## Task 4: Bound Python to the Legacy Bridge Only

**Files:**

- Modify: `apps/heiwa_cli/scripts/agents/worker_manager.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/heiwaclaw/adapters/acp.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/config.py`
- Test: `apps/heiwa_core/tests/legacy_bridge.rs`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write the failing legacy bridge test**

Add `apps/heiwa_core/tests/legacy_bridge.rs` that proves:

- `/ws/worker/legacy` accepts the bounded old frames
- the adapter translates them into canonical v1 runtime behavior
- Python does not need to own lease/session truth

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p heiwa-core --test legacy_bridge -- --nocapture
```

Expected: FAIL until the legacy boundary is fully explicit.

- [ ] **Step 3: Remove remaining authority assumptions from Python**

Change the Python bridge files so they only:

- connect
- translate
- surface errors

They must not invent lease state, extend policy, or infer canonical routing state.

- [ ] **Step 4: Make the legacy bridge test pass**

Update the Rust legacy path and Python adapters minimally until the test passes.

- [ ] **Step 5: Re-run Python syntax and legacy bridge verification**

Run:

```bash
python3 -m py_compile apps/heiwa_cli/scripts/agents/worker_manager.py packages/heiwa_sdk/heiwa_sdk/heiwaclaw/adapters/acp.py packages/heiwa_sdk/heiwa_sdk/config.py
cargo test -p heiwa-core --test legacy_bridge -- --nocapture
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_cli/scripts/agents/worker_manager.py packages/heiwa_sdk/heiwa_sdk/heiwaclaw/adapters/acp.py packages/heiwa_sdk/heiwa_sdk/config.py apps/heiwa_core/tests/legacy_bridge.rs
git commit -m "refactor: reduce python worker surfaces to legacy bridge"
```

## Task 5: Make the TypeScript Workspace the Real Web Foundation

**Files:**

- Modify: `package.json`
- Modify: `apps/heiwa_web/package.json`
- Modify: `apps/heiwa_web/tsconfig.json`
- Modify: `packages/heiwa_bindings/typescript/package.json`
- Modify: active files under `apps/heiwa_web/clients/web/assets/`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write the failing TypeScript migration test/check**

Pick one active JS surface under `apps/heiwa_web/clients/web/assets/` and create the smallest typecheck-oriented migration target. The first test is the workspace `typecheck` command itself failing against the newly introduced TS-backed surface.

- [ ] **Step 2: Run the typecheck to verify it fails**

Run:

```bash
npm install --ignore-scripts
npm run typecheck
```

Expected: FAIL once the first JS file is deliberately moved into typed surface area without full implementation.

- [ ] **Step 3: Convert the smallest meaningful web asset**

Move one real active web asset into a TS-aware module or add a typed wrapper that the static shell can consume. Do not attempt the full shell rewrite here.

- [ ] **Step 4: Make the typecheck pass**

Update package exports/imports and workspace settings minimally until `npm run typecheck` passes again.

- [ ] **Step 5: Commit**

```bash
git add package.json apps/heiwa_web packages/heiwa_bindings/typescript
git commit -m "feat: establish typed web surface on npm workspace"
```

## Task 6: Tighten Operator and CI Trust Around the New Spine

**Files:**

- Modify: `.github/workflows/deploy.yml`
- Modify: `scripts/audit_operator_machine.sh`
- Modify: `docs/standards/runtime-baseline.md`
- Modify: `README.md`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write the failing baseline expectation**

Add or extend a CI/runtime-baseline check so it fails if:

- Node 24 pin drifts
- Rust 1.93.1 pin drifts
- Python 3.14 pin drifts from the already-live baseline
- TS workspace exists but lockfile/typecheck are missing from CI policy

- [ ] **Step 2: Run the check to verify it fails**

Run the narrowest affected script or workflow lint command first.

- [ ] **Step 3: Add dependency-audit hooks**

Wire in the minimum practical checks for:

- `cargo audit` when available in CI
- npm workspace install/typecheck once the lockfile is real

Do not add flaky network-heavy jobs blindly.

- [ ] **Step 4: Update docs to match the trust model**

Make sure runtime and operator docs say:

- Rust authority
- STDB truth
- Python bridge only
- TS surface
- Node 24 baseline

- [ ] **Step 5: Re-run full verification**

Run:

```bash
cargo test -p heiwa-core --quiet
npm run typecheck
bash scripts/check_runtime_baseline.sh
actionlint .github/workflows/deploy.yml
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/deploy.yml scripts/audit_operator_machine.sh docs/standards/runtime-baseline.md README.md
git commit -m "chore: tighten ci and operator trust around execution spine"
```

## Final Verification Checklist

- [ ] `cargo test -p heiwa-core --quiet`
- [ ] `cargo test -p heiwa-core --test worker_mesh -- --nocapture`
- [ ] `cargo test -p heiwa-core --test run_receipts -- --nocapture`
- [ ] `cargo test -p heiwa-core --test legacy_bridge -- --nocapture`
- [ ] `rg -n "capability_leases|task_dispatches|nodes" apps/heiwa_core/src/runtime apps/heiwa_core/src/stdb`
- [ ] `python3 -m py_compile apps/heiwa_cli/scripts/agents/worker_manager.py packages/heiwa_sdk/heiwa_sdk/heiwaclaw/adapters/acp.py packages/heiwa_sdk/heiwa_sdk/config.py`
- [ ] `npm install --ignore-scripts`
- [ ] `npm run typecheck`
- [ ] `bash scripts/check_runtime_baseline.sh`
- [ ] `bash scripts/check_heiwa_core_dockerfile.sh`
- [ ] `actionlint .github/workflows/deploy.yml`
