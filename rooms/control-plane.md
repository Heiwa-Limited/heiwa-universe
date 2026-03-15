# Control Plane Room

Load this room for:

- proposal lifecycle work
- routing / lease / approval design
- STDB state authority changes
- hub control-plane behavior

## What This Room Owns

- Railway as the control-plane host
- SpacetimeDB as the authoritative state layer
- proposal, assignment, consent, approval, and lease state
- `HeiwaCells` and `HeiwaBench` as control-plane surfaces

## Live Surfaces

- Typed route contracts in `packages/heiwa_protocol/heiwa_protocol/routing.py`
- Broker enrichment in `apps/heiwa_hub/cognition/enrichment.py`
- Gateway dispatch in `packages/heiwa_sdk/heiwa_sdk/heiwaclaw.py`
- MCP/HTTP surface in `apps/heiwa_hub/mcp_server.py`
- STDB module in `apps/heiwa_hub/spacetimedb/src/lib.rs`

## Current Reality

- STDB now owns route decisions, runs, nodes, liveness state, proposal lifecycle state, approval records, and capability lease records.
- `HeiwaBench` now gates route and cell behavior through checked-in suites under `config/swarm/benchmarks/`.
- `HeiwaCells` now materializes identity profiles into a real catalog surface.
- The hub proposal HTTP surface now writes through the STDB bridge for create / claim / consent / heartbeat flows when `HEIWA_STATE_BACKEND=spacetimedb`.

## Transitional Boundary

Proposal / lease / RFC is not fully migrated yet.

Today:

- `capability_leases`, `approval_requests`, and `approval_decisions` now exist in the STDB module and Python bridge.
- `packages/heiwa_sdk/heiwa_sdk/db.py` now routes proposal assignment, claim, consent, heartbeat, routing queries, and lease issuance through STDB fast paths when enabled.
- Routing and assignment selection still runs in `packages/heiwa_sdk/heiwa_sdk/tick.py`, but STDB is now the state authority underneath that scheduler.
- Public WebSockets currently stream status, not proposal lifecycle events.
- Approval state also exists in-memory in `apps/heiwa_hub/cognition/approval.py`.

## Next Mandatory Cut

Replace polling with subscriptions:

- nodes subscribe to proposal assignments targeted to them
- nodes subscribe to active / revoked capability leases
- operator surfaces subscribe to approval requests and decisions
- the hub stops depending on `tick.py` for the proposal assignment fast path
