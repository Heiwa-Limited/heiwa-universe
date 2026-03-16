# Phase 6: Portability & Final Polish — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure the system is portable, efficient, and free of dead code. Finalize the boot sequence and provide a clean handover.

**Architecture:**
- **Environment Parity**: Single `start.sh` that works on local Mac, Linux, and Railway.
- **Cleanup**: Remove remaining multi-backend logic and dead NATS configuration.
- **Pruning**: Implement TTL-based pruning for memory and execution logs in STDB.

**Tech Stack:** Python 3.14, Bash, SpacetimeDB

---

## Chunk 1: Boot Sequence & Portability

### Task 1: Refine `start.sh`

- [ ] **Step 1: Default to `HEIWA_STATE_BACKEND=spacetimedb`** if not explicitly set.
- [ ] **Step 2: Add STDB pre-flight check**: ensure server is reachable before starting agents.
- [ ] **Step 3: Auto-detect database name** (identity) from `spacetime.json` if available.

### Task 2: Environment Audit

- [ ] **Step 1: Update `.env.example`** to remove obsolete variables (#12).
- [ ] **Step 2: Ensure `OLLAMA_BASE_URL`** is correctly passed to all components.

---

## Chunk 2: Cleanup & Maintenance

### Task 3: STDB Data Pruning

- [ ] **Step 1: Implement `prune_memory()`** in `MemoryService` to delete embeddings older than `ttl_hours`.
- [ ] **Step 2: Implement `prune_logs()`** in `Database` to keep only the last 1000 execution records.
- [ ] **Step 3: Schedule pruning** in `CaptainAgent._captain_tick`.

### Task 4: Final Dead Code Removal

- [ ] **Step 1: Remove `packages/heiwa_sdk/heiwa_sdk/claw_adapter.py`** (replaced by `heiwaclaw` package).
- [ ] **Step 2: Remove obsolete tests** that reference deleted multi-backend code.

---

## Chunk 3: Final Verification

### Task 5: End-to-End Smoke Test

- [ ] **Step 1: Start system via `heiwa start`** and verify full boot sequence.
- [ ] **Step 2: Verify `heiwa /status`** shows all services OK and STDB active.
- [ ] **Step 3: Perform a final `pytest`** run of the entire suite.
