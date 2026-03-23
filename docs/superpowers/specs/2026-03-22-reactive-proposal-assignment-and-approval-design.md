# Reactive Proposal Assignment And Approval Design

**Date:** 2026-03-22
**Status:** Approved for implementation

## Goal

Remove control-plane polling from the proposal assignment fast path and the approval operator surface without touching alert scanning or RFC publishing in `tick.py`.

## Scope

This cut replaces two transitional behaviors:

1. Proposal-to-node assignment logic living only in `packages/heiwa_sdk/heiwa_sdk/tick.py`
2. Approval visibility sourced from the in-memory `ApprovalRegistry` plus periodic operator snapshot pushes

This cut does **not** change:

- alert scanning
- proposal generation from alerts
- RFC publishing
- Discord notifications
- the broader Captain/work-loop roadmap

## Current Reality

### Proposal assignment

- The authoritative proposal state is already in SpacetimeDB.
- Assignment logic exists in `tick.py::router_tick()`.
- Nodes become eligible through the `nodes` table and later claim assigned proposals through the HTTP proposal lifecycle endpoints.
- No hub-side reactive trigger exists when:
  - a new routable proposal is created
  - a proposal is approved
  - a node becomes online or refreshes liveness

### Approval state

- `Spine` still uses `ApprovalRegistry` as the live source of truth needed to resume or reject held tasks.
- SpacetimeDB already has `approval_requests` and `approval_decisions` tables plus reducers.
- `/approvals` and `/ws/operator` currently derive approval data from process-local memory, not STDB.
- `/ws/operator` sends snapshots every 2 seconds, which is polling over a websocket rather than event-driven delivery.

## Target Design

### 1. Reactive proposal assignment service

Add a small shared service that contains the existing routing logic currently embedded in `tick.py::router_tick()`.

Responsibilities:

- load routable proposals from STDB
- parse `execution_targeting`
- compute eligible nodes using the existing `Database.get_eligible_nodes(...)`
- assign, re-queue, or expire proposals using the existing DB/STDB methods
- return structured counts for tests and observability

Trigger points:

- after proposal creation through `/proposals`
- after approval via `/proposals/{proposal_id}/consent` when the decision is `APPROVE`
- after node liveness updates through `/nodes/{node_id}/heartbeat`
- after worker websocket register / heartbeat / unregister in the hub server

This removes the need for a periodic server-side poller to decide assignments.

### 2. Worker registration updates STDB node state

When a remote worker connects through `/ws/worker`, the hub should immediately mirror that worker into STDB:

- `upsert_node_heartbeat(...)` on register and heartbeat
- `set_node_status(..., "ONLINE")` on register
- `set_node_status(..., "OFFLINE")` on unregister

The mirrored node record must preserve enough metadata for `get_eligible_nodes(...)` to work:

- capability list
- privilege tier
- runtime / node identity details in `meta`

### 3. STDB-backed approval operator view

Keep `ApprovalRegistry` for in-process task suspension/resume, but stop using it as the operator-facing source.

`Spine` should persist:

- an `approval_request` row when it holds a task
- an `approval_decision` row when an operator approves or rejects
- an updated `approval_request` status of `EXPIRED` when the hold times out

Operator-facing reads should prefer STDB when available:

- `/approvals`
- `_operator_snapshot_payload()`
- approval rendering inside `/ws/operator`

Fallback to `ApprovalRegistry` remains for non-STDB backends and existing compatibility tests.

### 4. Event-driven operator websocket

Replace the fixed 2-second send loop in `/ws/operator` with push-on-change behavior:

- send one initial snapshot on connect
- publish fresh snapshots only when task / approval state changes

This preserves the current payload shape and avoids breaking the existing operator UI.

## Data Flow

### Proposal assignment

1. Proposal is created or approved, or a node heartbeat/register event arrives
2. Reactive assignment service runs once
3. Eligible node selection uses current STDB node rows
4. Proposal state is updated in STDB (`ASSIGNED`, `QUEUED`, or `EXPIRED`)
5. Node can claim through the existing proposal lifecycle API

### Approval flow

1. `Spine` holds a task for manual approval
2. `ApprovalRegistry` keeps the in-process hold state
3. STDB records the approval request for operator visibility
4. Operator approves/rejects through the existing endpoint
5. STDB records the decision, `Spine` resumes/rejects the task, and operator websocket subscribers receive a pushed snapshot

## Error Handling

- Reactive assignment is best-effort at each trigger point; failures are logged and surfaced in test assertions, but do not crash the HTTP or websocket request path.
- Worker STDB sync failures must not break worker registration, but they should be logged clearly.
- Approval persistence failures must not silently corrupt task behavior; task hold/resume still follows `ApprovalRegistry`, with STDB persistence treated as a required observability/control-plane write when STDB is enabled.

## Testing Strategy

1. Add a focused test for the extracted reactive assignment service
2. Add a hub/operator approval-source test proving operator reads come from STDB when available
3. Extend approval/task e2e coverage only where needed to avoid broad churn
4. Run the targeted hub tests plus the existing proposal lifecycle test

## Out Of Scope

- Native STDB client subscriptions
- node-side websocket subscriptions for proposal assignments
- capability lease enforcement changes
- replacement of `ApprovalRegistry`
- Captain / work-loop orchestration changes
