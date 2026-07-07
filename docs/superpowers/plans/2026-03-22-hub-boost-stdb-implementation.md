# Hub/Boost STDB-Mediated Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a resilient Hub/Boost connection via STDB with durable spooling, capability-based routing, and full-modal Discord.

**Architecture:** STDB as the shared control plane, local MacBook as Tier 0 execution (Ollama), Railway as Hub (Orchestration). Local JSONL spooling for STDB outages.

**Tech Stack:** Python (Hub/SDK), Rust (STDB Reducers), SpacetimeDB (WebSocket/SQL), Discord API.

---

### Phase 1: STDB Dialect & Bridge Resilience

**Files:**

- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- Test: `apps/heiwa_hub/tests/test_stdb_dialect_cleanup.py`

- [ ] **Step 1: Write failing test for `IN` clause cleanup**
- [ ] **Step 2: Implement `get_routable_proposals` using compatible SQL**

```python
# Replace status IN ('APPROVED', 'QUEUED') with:
"WHERE (status = 'APPROVED' OR status = 'QUEUED') "
```

- [ ] **Step 3: Add `call_with_retry` helper with exponential backoff (1s, 2s, 4s)**
- [ ] **Step 4: Wrap `upsert_node_heartbeat` and `finish_cell_run` in retry logic**
- [ ] **Step 5: Verify tests pass and commit**

### Phase 2: Durable Spooling & Railway Volume Binding

**Files:**

- Create: `runtime/spool/.gitkeep`
- Modify: `apps/heiwa_hub/mcp_server.py`
- Modify: `apps/heiwa_hub/main.py`

- [ ] **Step 1: Initialize `runtime/spool/` directory at startup in `main.py`**
- [ ] **Step 2: Implement `_spool_to_dead_letter(task_data)` in `mcp_server.py`**

```python
with open("runtime/spool/dead_letter_proposals.jsonl", "a") as f:
    f.write(json.dumps(task_data) + "\n")
```

- [ ] **Step 3: Modify `db.add_proposal` calls to spool on retry exhaustion**
- [ ] **Step 4: Test durability by simulating STDB 502 and checking disk**
- [ ] **Step 5: Commit**

### Phase 3: Proposal/Cell-Run State Machine & Watchdog

**Files:**

- Modify: `apps/heiwa_hub/agents/spine.py`
- Modify: `apps/heiwa_hub/orchestrator.py`

- [ ] **Step 1: Implement "Audit Watchdog" loop in `Spine` (60s tick)**
- [ ] **Step 2: Add logic to scan for `ASSIGNED` tasks with stale heartbeats**
- [ ] **Step 3: Implement `requeue_stale_proposals` (revert `ASSIGNED -> QUEUED`)**
- [ ] **Step 4: Update `ComputeRouter` to respect `assignment_expires_at`**
- [ ] **Step 5: Verify reconciliation in local dev and commit**

### Phase 4: Capability-Based Routing (Registry-Linked)

**Files:**

- Modify: `apps/heiwa_hub/orchestrator.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/main.py`

- [ ] **Step 1: Connect `ComputeRouter` to STDB `model_tiers` table**
- [ ] **Step 2: Implement Class-Preserving Fallback logic (Class 3 -> Class 3)**
- [ ] **Step 3: Add `REQUIRES_HUMAN_OVERSIGHT` policy check**
- [ ] **Step 4: Test fallback from MacBook (Offline) -> Gemini 3.1 Pro (Online)**
- [ ] **Step 5: Commit**

### Phase 5: Full-Modal Discord & Signed Footers

**Files:**

- Modify: `apps/heiwa_hub/agents/messenger.py`

- [ ] **Step 1: Implement HMAC-based footer signing in `Messenger`**
- [ ] **Step 2: Add `[Retry Spool]` button handler (routes to Operator DM)**
- [ ] **Step 3: Add `[Force Requeue]` and `[View Telemetry]` buttons to task embeds**
- [ ] **Step 4: Label messages with `[⚡ Boost]` vs `[☁️ Cloud]` headers**
- [ ] **Step 5: Verify UI in Discord and commit**

### Phase 6: Railway Environment & Memory Gating

**Files:**

- Modify: `packages/heiwa_sdk/heiwa_sdk/memory.py`
- Modify: `apps/heiwa_hub/start.sh`

- [ ] **Step 1: Remove hardcoded `127.0.0.1` default in `MemoryService`**
- [ ] **Step 2: Implement autonomous Ollama fallback to Cloud Embeddings**
- [ ] **Step 3: Add `PORTABLE` mode flag for Tailscale/TPM missing on Railway**
- [ ] **Step 4: Verify Hub boots cleanly on Railway via logs**
- [ ] **Step 5: Commit**

### Phase 7: Orphaned Cell-Run Recovery

**Files:**

- Modify: `apps/heiwa_hub/mcp_server.py`

- [ ] **Step 1: Wrap `finish_cell_run` in 404/530 check**
- [ ] **Step 2: Ensure `mission_result` and `artifacts` are written even if cell-run is missing**
- [ ] **Step 3: Add regression test for "Orphaned but Completed" mission**
- [ ] **Step 4: Verify and commit**
