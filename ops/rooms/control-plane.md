# Control Plane Room

Load this room for:

- proposal lifecycle work
- routing / lease / approval design
- STDB state authority changes
- hub control-plane behavior

## What This Room Owns

- MacBook `heiwa` runtime as the current control-plane host
- local state as current owner truth, with SpacetimeDB as sync/adjudication plane
- proposal, assignment, consent, approval, and lease state
- `HeiwaCells` and `HeiwaBench` as control-plane surfaces

## Live Surfaces

- Rust routing and execution kernel in `apps/heiwa_core/`
- DREX orchestration and STDB-facing runtime work in `apps/heiwa_orchestrator/`
- Typed route contracts in `packages/heiwa_protocol/heiwa_protocol/routing.py`
- Gateway dispatch in `packages/heiwa_sdk/heiwa_sdk/heiwaclaw.py`
- STDB evidence and fallback crate in `crates/heiwa_stdb/`

## Current Reality

- Local Heiwa state now owns current user functionality; STDB mirrors/adjudicates route decisions, runs, nodes, liveness state, proposal lifecycle state, approval records, and capability lease records when enabled.
- `HeiwaBench` now gates route and cell behavior through checked-in suites under `config/swarm/benchmarks/`.
- `HeiwaCells` now materializes identity profiles into a real catalog surface.
- The legacy Hub proposal HTTP surface is quarantined; use it as migration/reference unless explicitly promoting that path.

## Transitional Boundary

Proposal / lease / RFC is not fully migrated yet.

Today:

- `capability_leases`, `approval_requests`, and `approval_decisions` now exist in the STDB module and Python bridge.
- `packages/heiwa_sdk/heiwa_sdk/db.py` now routes proposal assignment, claim, consent, heartbeat, routing queries, and lease issuance through STDB fast paths when enabled.
- Routing and assignment selection still runs in `packages/heiwa_sdk/heiwa_sdk/tick.py`, but STDB is now the state authority underneath that scheduler.
- Public WebSockets currently stream status, not proposal lifecycle events.

## Next Mandatory Cut

Replace polling with subscriptions:

- nodes subscribe to proposal assignments targeted to them
- nodes subscribe to active / revoked capability leases
- operator surfaces subscribe to approval requests and decisions
- the hub stops depending on `tick.py` for the proposal assignment fast path
