# Design Spec: Hub/Boost STDB-Mediated Architecture (2026-03-22)

## Status: APPROVED (v5)

**Date:** Sunday, March 22, 2026
**Author:** Gemini CLI (Heiwa Class 3 Executor)
**Target:** Heiwa Monorepo (Railway Hub + Local Boost Node)

## 1. Objective

Establish a secure, resilient, and autonomous communication layer between the Railway Hub and MacBook Boost Node using SpacetimeDB as the primary event bus. This spec prioritizes local inference while defining strict durable-mode behaviors for production stability.

## 2. Proposal & Cell-Run State Machine

### 2.1 Reconciled Lifecycle

Proposals follow a deterministic state machine that incorporates existing STDB legacy statuses:

1. **QUEUED:** Initial state for automated tasks.
2. **APPROVED:** State for tasks that have passed a human/policy consent gate.
3. **ASSIGNED:** `ComputeRouter` has selected a node. `assigned_node_id` and `assignment_expires_at` are set.
4. **CLAIMED (RUNNING):** Node picks up task. `claimed_at` is set. Node owns the heartbeat.
5. **COMPLETED / FAILED:** Final execution state.
6. **EXPIRED / REQUEUED:** If `assignment_expires_at` passes without a `claimed_at`, Hub reverts to **QUEUED** (or **APPROVED** if previously consented).

### 2.2 Ownership & Recovery

- **Hub:** Owns `QUEUED/APPROVED -> ASSIGNED` and `EXPIRED -> REQUEUED`.
- **Node:** Owns `ASSIGNED -> CLAIMED` and `CLAIMED -> COMPLETED`.
- **Reconciliation:** A Hub-side "Audit Watchdog" scans for `ASSIGNED` tasks with stale heartbeats every 60s.

## 3. Durable Mode & Failure Handling (The "Resilient Path")

### 3.1 Hardened Spooling (Railway Persistence)

- **Persistence:** The `runtime/spool/` directory **MUST** be mounted to a Railway Persistent Volume (or similar durable mount point).
- **Spool Rule:** If STDB write fails after 3 retries, the Hub writes to `runtime/spool/dead_letter_proposals.jsonl`.
- **Notification:** Hub sends a **Direct Message (DM)** to the operator via the `Messenger` agent with a `[Retry Spool]` button.

### 3.2 Cell-Run Reconciliation (Adjacency Rule)

- **Idempotency:** `finish_cell_run` checks for run existence.
- **Adjacent State:** If a cell-run is "Orphaned" (404/530), the Hub **MUST** still attempt to write the `mission_result` and `artifacts`. Mission-level truth supersedes execution-level logging.

### 3.3 Runtime Gating (Railway)

- **Attestation:** If TPM/Tailscale is missing, proceed in `PORTABLE` mode.
- **Ollama:** Fallback to **Tier 1 (Cloud Flash)** autonomously if local endpoint is down.

## 4. Capability-Based Routing & Fallback

The `ComputeRouter` is dynamically linked to the SpacetimeDB `model_tiers` table (bootstrapped from `config/seeds/model_tiers.json`), which serves as the runtime source of truth:

1. **Capability: `local_inference`**
   - **Preferred:** `qwen3.5:4b` (Capability Class 2).
   - **Fallback:** Autonomous Tier 1 (Cloud Flash).
2. **Capability: `surgical_code`**
   - **Preferred:** `claude/sonnet-4-6` (Capability Class 2).
   - **Fallback:** `gemini-cli/gemini-3.1-pro` (Capability Class 3).
3. **Capability: `high_reasoning`**
   - **Preferred:** `codex/gpt-5.4` / `claude/opus-4-6` (Capability Class 3).
   - **Fallback Policy:** If preferred is unavailable, fallback to the **best available non-flash reasoning model** (e.g., `gemini-cli/gemini-3.1-pro`, Capability Class 3) autonomously, unless a `REQUIRES_HUMAN_OVERSIGHT` flag is present.

## 5. Discord Interface (Full Modal)

- **Signed Footers:** Hash of `(NodeID + ModelID + Timestamp)`.
- **Direct Messaging:** Critical errors (Spooling, Auth Failures) are routed to Operator DMs.
- **Explicit Labeling:** `[⚡ Boost]` vs `[☁️ Cloud]`.

## 6. Success & Acceptance Criteria (Production-Driven)

This design is successful when:

1. **Survives Restart:** A spooled task in `runtime/spool/` persists across a Railway container redeploy and can be recovered.
2. **Orphaned Cleanup:** An "Orphaned" execution still results in a closed Mission.
3. **Status Integrity:** The Hub successfully queries both `APPROVED` and `QUEUED` tasks for assignment.
4. **Class-Preserving Fallback:** A `high_reasoning` task falls back to a Class 3 model (e.g., `gemini-3.1-pro`) instead of dropping to Class 2 (Flash) when possible.

## 7. Relationship to Existing Docs

This spec **supersedes** all previous "Mesh" or "Sovereignty" drafts. It is the authoritative design for the March 2026 architecture.
