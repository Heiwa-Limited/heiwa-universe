# Heiwa Native Session Delegation A1+A2 Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a provider-neutral native-session substrate and ship a Claude Code pilot that can execute scoped delegated tasks with local-first receipts, STDB projection, and honest routing feedback.

**Architecture:** Keep `ProviderAdapter` as the stateless path, add `SessionProvider` plus execution-surface capability metadata, store live session truth under `~/.heiwa/sessions/<id>/`, project coarse state through `task_dispatches` and run/artifact receipts, and launch Claude Code in captured or interactive mode using a stable Heiwa-owned session id.

**Tech Stack:** Rust, existing `heiwa_provider` crate, new Rust session crate, SpacetimeDB reducers, generated bindings, Claude Code CLI

**Optimization Doctrine:** Local Gemma/Qwen stay first for classification, summarization, bounded coding, and verification. Claude, Gemini, and Codex are escalation surfaces chosen only when native tool use, reasoning depth, or accuracy justify the spend.

---

### Task 1: Add Session Traits and Surface Capability Metadata

**Files:**

- Modify: `crates/heiwa_provider/src/adapter.rs`
- Modify: `crates/heiwa_provider/src/registry.rs`
- Modify: `crates/heiwa_provider/src/lib.rs`
- Create: `crates/heiwa_provider/src/session.rs`
- Create: `crates/heiwa_provider/tests/session_surface_capabilities.rs`

- [ ] **Step 1: Write failing capability tests**

Create `crates/heiwa_provider/tests/session_surface_capabilities.rs` covering:

- `claude-code` exposes native-session capability
- `codex` exposes native-session capability
- `google-gemini-cli` exposes native-session capability
- `ollama` does not expose native-session capability
- Claude reports `supports_custom_session_id = true`
- Claude reports `supports_cost_budget = true`

- [ ] **Step 2: Run the new test and confirm failure**

Run:

```bash
cargo test -p heiwa-provider session_surface_capabilities
```

Expected: FAIL because no session capability metadata exists yet.

- [ ] **Step 3: Add `SessionProvider` and capability types**

Create `crates/heiwa_provider/src/session.rs` with:

- `ExecutionSurfaceCapabilities`
- `SessionSpec`
- `SessionHandle`
- `SessionStatus`
- `CollectedArtifacts`
- `SessionProvider`

Keep `ProviderAdapter` intact in `adapter.rs`.

- [ ] **Step 4: Add registry-level provider handles**

Modify `registry.rs` so provider discovery can return a `ProviderHandle` containing:

- stateless adapter
- optional native session provider
- explicit surface capability metadata

- [ ] **Step 5: Re-run capability verification**

Run:

```bash
cargo test -p heiwa-provider session_surface_capabilities
```

Expected: PASS

### Task 2: Create a Local-First Session Manager Crate

**Files:**

- Create: `crates/heiwa_session/Cargo.toml`
- Create: `crates/heiwa_session/src/lib.rs`
- Create: `crates/heiwa_session/src/spec.rs`
- Create: `crates/heiwa_session/src/store.rs`
- Create: `crates/heiwa_session/src/manager.rs`
- Create: `crates/heiwa_session/src/stats.rs`
- Create: `crates/heiwa_session/tests/session_store_roundtrip.rs`
- Create: `crates/heiwa_session/tests/model_stats_rollup.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Write failing local-store tests**

Create:

- `crates/heiwa_session/tests/session_store_roundtrip.rs`
- `crates/heiwa_session/tests/model_stats_rollup.rs`

Assertions should cover:

- session directory creation under `~/.heiwa/sessions/<id>/`
- `spec.json` and `status.json` round-trip correctly
- artifacts can be appended idempotently
- rolling 20-run model stats derive from local receipts

- [ ] **Step 2: Run the new crate tests and confirm failure**

Run:

```bash
cargo test -p heiwa-session
```

Expected: FAIL because the crate does not exist yet.

- [ ] **Step 3: Implement the local session store**

Add:

- file-backed session state helpers
- `Captured` vs `Interactive` status persistence
- review status tracking
- local receipt and artifact manifests

- [ ] **Step 4: Implement model stat rollups**

Add rolling local aggregation for:

- success rate
- average latency
- p95 latency

Output should be ready to feed `update_model_tier_stats`.

- [ ] **Step 5: Re-run crate verification**

Run:

```bash
cargo test -p heiwa-session
```

Expected: PASS

### Task 3: Align STDB Projection With Delegated Sessions

**Files:**

- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `apps/heiwa_core/src/stdb/mod.rs`
- Modify: `apps/heiwa_hub/tests/test_phase3_integration.py`
- Create: `apps/heiwa_hub/tests/test_delegated_task_dispatch.py`

- [ ] **Step 1: Write failing STDB projection tests**

Create `apps/heiwa_hub/tests/test_delegated_task_dispatch.py` covering:

- `task_dispatches` accepts `pending_review`, `timed_out`, `cancelled`, and `rejected`
- delegated session artifacts persist with `mission_id = task_id`
- delegated run receipts use `mode = "native_session"`

- [ ] **Step 2: Run the new STDB-focused tests and confirm failure**

Run:

```bash
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_delegated_task_dispatch.py
```

Expected: FAIL because delegated-session status and receipt assumptions are not encoded yet.

- [ ] **Step 3: Extend the STDB bridge and reducer vocabulary**

Modify STDB surfaces so:

- `task_dispatches.status` documents the expanded delegated-session states
- `PersistedRunReceipt` can be used cleanly for `native_session`
- artifact registration stays idempotent for repeated sync

- [ ] **Step 4: Re-run STDB projection verification**

Run:

```bash
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_delegated_task_dispatch.py
```

Expected: PASS

### Task 4: Implement the Claude Code Session Provider

**Files:**

- Modify: `crates/heiwa_provider/src/providers/claude_code.rs`
- Modify: `crates/heiwa_provider/src/providers/mod.rs`
- Create: `crates/heiwa_provider/tests/claude_session_provider.rs`

- [ ] **Step 1: Write a failing Claude session-provider test**

Create `crates/heiwa_provider/tests/claude_session_provider.rs` using a fixture or fake `claude` binary to assert:

- `start_session` launches Claude with `--session-id`
- captured mode uses `--output-format stream-json`
- captured mode enables `--include-hook-events`
- `resume_session` uses `--resume <session_id>`
- `--max-budget-usd` is only included when configured
- `--worktree` is never used by the adapter

- [ ] **Step 2: Run the Claude provider test and confirm failure**

Run:

```bash
cargo test -p heiwa-provider claude_session_provider
```

Expected: FAIL because Claude is still implemented as a one-shot stateless adapter only.

- [ ] **Step 3: Implement `SessionProvider` for Claude**

Expand `claude_code.rs` so Claude supports:

- captured launch
- interactive launch
- resume
- cancel
- structured event collection
- artifact collection hooks

Keep the existing `ProviderAdapter::send()` path intact for backward compatibility.

- [ ] **Step 4: Re-run Claude provider verification**

Run:

```bash
cargo test -p heiwa-provider claude_session_provider
```

Expected: PASS

### Task 5: Add Minimal Worktree Isolation For Claude Write Sessions

**Files:**

- Modify: `crates/heiwa_session/src/manager.rs`
- Create: `crates/heiwa_session/src/worktree.rs`
- Create: `crates/heiwa_session/tests/worktree_isolation.rs`

- [ ] **Step 1: Write a failing worktree-isolation test**

Create `crates/heiwa_session/tests/worktree_isolation.rs` covering:

- `IsolatedWrite` creates a new worktree
- the worktree path is recorded in `worktree.json`
- the base repo stays untouched during the session
- completion emits a diff artifact

- [ ] **Step 2: Run the worktree test and confirm failure**

Run:

```bash
cargo test -p heiwa-session worktree_isolation
```

Expected: FAIL because worktree helpers do not exist yet.

- [ ] **Step 3: Implement minimal A2 worktree support**

Add:

- worktree creation for write-capable Claude sessions
- post-run diff generation
- `pending_review` status for write sessions

This is the minimum safe substrate for A2, not the full A4 generalization.

- [ ] **Step 4: Re-run worktree verification**

Run:

```bash
cargo test -p heiwa-session worktree_isolation
```

Expected: PASS

### Task 6: Add a Pilot `heiwa` Entry Point For Delegated Claude Sessions

**Files:**

- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `crates/heiwa_loop/src/lib.rs`
- Create: `crates/heiwa_loop/tests/native_session_routing.rs`

- [ ] **Step 1: Write a failing integration test**

Create `crates/heiwa_loop/tests/native_session_routing.rs` asserting:

- a scoped delegated task can request `native_session`
- routing filters to session-capable surfaces
- tasks that remain within acceptable local quality bounds stay on local Gemma/Qwen stateless execution
- Claude pilot tasks create a local session directory and task dispatch row

- [ ] **Step 2: Run the integration test and confirm failure**

Run:

```bash
cargo test -p heiwa-loop native_session_routing
```

Expected: FAIL because there is no delegated-session execution path yet.

- [ ] **Step 3: Wire a narrow pilot entry point**

Add a pilot path that:

- accepts a scoped delegated task spec
- routes only to a session-capable surface when requested
- launches the Claude session provider
- projects coarse state to STDB

This does not replace the loop controller yet. It adds a safe adjacent path.

- [ ] **Step 4: Re-run integration verification**

Run:

```bash
cargo test -p heiwa-loop native_session_routing
```

Expected: PASS

### Acceptance Conditions

- [ ] Local `qwen3.5` / `gemma4` remain the default low-cost execution tier for scoped work that does not require remote capability.
- [ ] Native-session escalation only happens when the task explicitly requires session semantics or local quality/risk is insufficient.
- [ ] Claude pilot success does not regress cost discipline by turning remote sessions into the default coding path.

### Task 7: Full Focused Verification

- [ ] **Step 1: Run Rust verification**

```bash
cargo test -p heiwa-provider session_surface_capabilities
cargo test -p heiwa-provider claude_session_provider
cargo test -p heiwa-session
cargo test -p heiwa-loop native_session_routing
```

- [ ] **Step 2: Run Python/STDB verification**

```bash
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_delegated_task_dispatch.py
```

- [ ] **Step 3: Smoke-test the pilot manually**

Run one read-only captured Claude delegated task and confirm:

- a task dispatch row exists
- local receipts were written
- artifacts were recorded
- model tier stats update after completion

Expected: PASS
