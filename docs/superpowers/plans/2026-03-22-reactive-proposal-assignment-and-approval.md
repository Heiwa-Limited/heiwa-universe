# Reactive Proposal Assignment And Approval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove `tick.py` from the proposal assignment fast path and stop operator approval visibility from depending on process-local polling.

**Architecture:** Extract the existing proposal routing logic into a shared reactive service and invoke it from proposal and node liveness events. Persist approval requests and decisions into SpacetimeDB while keeping `ApprovalRegistry` for in-process task suspension. Replace the `/ws/operator` fixed send loop with event-driven snapshot pushes without changing the external payload shape.

**Tech Stack:** Python 3.14, FastAPI, SpacetimeDB bridge, pytest

---

### Task 1: Add the failing reactive assignment test

**Files:**

- Create: `apps/heiwa_hub/tests/test_reactive_assignment.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/proposal_dispatch.py`

- [ ] **Step 1: Write the failing test**

Create a focused test that seeds:

- one online eligible node
- one approved proposal requiring that node capability/privilege tier

Assert that the new dispatcher assigns the proposal and records the eligibility snapshot.

- [ ] **Step 2: Run test to verify it fails**

Run: `python apps/heiwa_hub/tests/test_reactive_assignment.py`
Expected: FAIL because the reactive dispatcher module/function does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Create `packages/heiwa_sdk/heiwa_sdk/proposal_dispatch.py` with a pure helper that:

- loads routable proposals
- computes eligible nodes
- assigns / queues / expires using the existing database façade

- [ ] **Step 4: Run test to verify it passes**

Run: `python apps/heiwa_hub/tests/test_reactive_assignment.py`
Expected: PASS

### Task 2: Wire reactive assignment into proposal and node events

**Files:**

- Modify: `packages/heiwa_sdk/heiwa_sdk/main.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/tick.py`
- Modify: `apps/heiwa_hub/mcp_server.py`

- [ ] **Step 1: Write the failing integration assertion**

Extend the reactive assignment test or add a second targeted test that proves node/proposal trigger points call the shared dispatcher.

- [ ] **Step 2: Run test to verify it fails**

Run: `python apps/heiwa_hub/tests/test_reactive_assignment.py`
Expected: FAIL because the trigger points are not wired.

- [ ] **Step 3: Write minimal implementation**

Update:

- `/proposals`
- `/proposals/{proposal_id}/consent` for approval decisions
- `/nodes/{node_id}/heartbeat`
- `/ws/worker` register / heartbeat / unregister

Also mirror worker liveness into STDB node state.

- [ ] **Step 4: Run test to verify it passes**

Run: `python apps/heiwa_hub/tests/test_reactive_assignment.py`
Expected: PASS

### Task 3: Add the failing STDB-backed approval view test

**Files:**

- Create: `apps/heiwa_hub/tests/test_operator_approval_source.py`
- Modify: `apps/heiwa_hub/mcp_server.py`
- Modify: `apps/heiwa_hub/agents/spine.py`

- [ ] **Step 1: Write the failing test**

Add a test that:

- provides a fake STDB-backed database with an approval request and approval decision
- leaves `ApprovalRegistry` empty
- asserts the operator approval serialization/listing uses STDB rows

- [ ] **Step 2: Run test to verify it fails**

Run: `python apps/heiwa_hub/tests/test_operator_approval_source.py`
Expected: FAIL because approval serialization currently reads only from the in-memory registry.

- [ ] **Step 3: Write minimal implementation**

Update `Spine` to persist approval request/decision state into STDB and update `mcp_server` approval serialization to prefer STDB rows with a registry fallback.

- [ ] **Step 4: Run test to verify it passes**

Run: `python apps/heiwa_hub/tests/test_operator_approval_source.py`
Expected: PASS

### Task 4: Replace operator websocket polling with event-driven pushes

**Files:**

- Modify: `apps/heiwa_hub/mcp_server.py`

- [ ] **Step 1: Write the failing test or narrow assertion**

Add a small test or assertion path proving operator snapshots are broadcast on change instead of only via a fixed sleep loop.

- [ ] **Step 2: Run test to verify it fails**

Run: `python apps/heiwa_hub/tests/test_operator_approval_source.py`
Expected: FAIL because `/ws/operator` still depends on the fixed 2-second loop.

- [ ] **Step 3: Write minimal implementation**

Add an operator snapshot broadcaster queue/set and trigger it from task status/result/progress updates.

- [ ] **Step 4: Run test to verify it passes**

Run: `python apps/heiwa_hub/tests/test_operator_approval_source.py`
Expected: PASS

### Task 5: Regression verification

**Files:**

- Test: `apps/heiwa_hub/tests/test_stdb_proposal_lifecycle.py`
- Test: `apps/heiwa_hub/tests/test_approval_gate_e2e.py`
- Test: `apps/heiwa_hub/tests/test_task_ingress_e2e.py`

- [ ] **Step 1: Run focused regression suite**

Run:

```bash
python apps/heiwa_hub/tests/test_reactive_assignment.py
python apps/heiwa_hub/tests/test_operator_approval_source.py
python apps/heiwa_hub/tests/test_stdb_proposal_lifecycle.py
python apps/heiwa_hub/tests/test_approval_gate_e2e.py
python apps/heiwa_hub/tests/test_task_ingress_e2e.py
```

Expected: PASS

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/specs/2026-03-22-reactive-proposal-assignment-and-approval-design.md \
  docs/superpowers/plans/2026-03-22-reactive-proposal-assignment-and-approval.md \
  packages/heiwa_sdk/heiwa_sdk/proposal_dispatch.py \
  packages/heiwa_sdk/heiwa_sdk/main.py \
  packages/heiwa_sdk/heiwa_sdk/tick.py \
  apps/heiwa_hub/mcp_server.py \
  apps/heiwa_hub/agents/spine.py \
  apps/heiwa_hub/tests/test_reactive_assignment.py \
  apps/heiwa_hub/tests/test_operator_approval_source.py
git commit -m "feat(control-plane): replace polling assignment and approval views"
```
