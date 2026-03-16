# Phase 3: Captain Autonomy & Self-Improvement — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform the Captain from a monitor into a proactive orchestrator that audits the repository, tunes model selection based on real-world performance, and dispatches fixing directives.

**Architecture:**
- **Repo Audit**: Captain triggers lint/test runs on boot and on git-related events.
- **Model Tuning**: Captain periodically aggregates `execution_memory` to update `model_tiers.last_success_rate`.
- **Directives**: Captain creates `captain_directives` in STDB for automated repairs (e.g., "fix lint").
- **Smart Routing**: `ComputeRouter` uses success rates to break ties between models.

**Tech Stack:** Python 3.14, SpacetimeDB

---

## Chunk 1: Captain Audit & Directives

### Task 1: Implement Repo Audit Logic

- [ ] **Step 1: Create `packages/heiwa_sdk/heiwa_sdk/audit.py`**
- [ ] **Step 2: Implement `run_audit()`**: executes `lint_config.py` and a smoke test of `pytest`.
- [ ] **Step 3: Return a structured `AuditResult`** (pass/fail, error logs).

### Task 2: Trigger Audits in `CaptainAgent`

- [ ] **Step 1: Update `CaptainAgent._captain_tick`** to run an audit every 10 minutes or on boot.
- [ ] **Step 2: Emit `Subject.LOG_INFO`** with audit results.
- [ ] **Step 3: Create a `captain_directives` record** in STDB if the audit fails (type: "repair_repo").

---

## Chunk 2: Self-Tuning Pipeline

### Task 3: Aggregate Execution Stats

- [ ] **Step 1: Add `get_execution_stats(model_id)` to `MemoryService`**
- [ ] **Step 2: Query `execution_memory`** for the last 20 runs of a model.
- [ ] **Step 3: Compute success rate and average latency.**

### Task 4: Update Model Tiers

- [ ] **Step 1: Update `CaptainAgent` proactive loop** to re-tune model stats every 5 minutes.
- [ ] **Step 2: Call `stdb.update_model_tier_stats`** with the aggregated data.

---

## Chunk 3: Success-Aware Routing

### Task 5: Update `ComputeRouter`

- [ ] **Step 1: Update `_select_model_from_tiers`** to include `last_success_rate` in the sorting logic.
- [ ] **Step 2: Sorting priority**: Intent match > Success Rate > Cost > Effort.
- [ ] **Step 3: Penalize models** with success rates below 0.5 by moving them to the bottom of the candidate list.

---

## Chunk 4: Directive Execution (Phase 3 scaffold)

### Task 6: Directives Handler

- [ ] **Step 1: Update `ExecutorAgent`** to listen for `Subject.CAPTAIN_DIRECTIVE` (new subject).
- [ ] **Step 2: If directive type is "repair_repo"**, dispatch a task to a Class 3 builder to fix the reported issues.

---

## Chunk 5: Verification

### Task 7: Integration Tests

- [ ] **Step 1: Verify Captain triggers audit and records results.**
- [ ] **Step 2: Verify `model_tiers` stats are updated after execution memory grows.**
- [ ] **Step 3: Verify router picks a different model if the cheapest one starts failing.**
