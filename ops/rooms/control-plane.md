# Control Plane Room

Load this room for:

- proposal lifecycle work
- routing / lease / approval design
- local evidence and Lance recall authority changes
- hub control-plane behavior

## What This Room Owns

- MacBook `heiwa` runtime as the current control-plane host
- local JSONL as current owner truth, with Lance as derived recall
- proposal, assignment, consent, approval, and lease state
- `HeiwaCells` and `HeiwaBench` as control-plane surfaces

## Live Surfaces

- Rust routing and execution kernel in `apps/heiwa_core/`
- DREX orchestration and evidence persistence in `apps/heiwa_orchestrator/`
- Typed route contracts in `packages/heiwa_protocol/heiwa_protocol/routing.py`
- Gateway dispatch in `packages/heiwa_sdk/heiwa_sdk/heiwaclaw.py`
- shared journal service in `crates/heiwa_evidence/`

## Current Reality

- Local Heiwa state owns current user functionality and records route decisions, runs, nodes, liveness, approvals, and leases through the local evidence service.
- `HeiwaBench` now gates route and cell behavior through checked-in suites under `config/swarm/benchmarks/`.
- `HeiwaCells` now materializes identity profiles into a real catalog surface.
- The legacy Hub proposal HTTP surface is quarantined; use it as migration/reference unless explicitly promoting that path.

## Transitional Boundary

Proposal / lease / RFC is not fully migrated yet.

Today:

- Rust protocol and evidence types own the supported lease and approval contracts.
- Python database and scheduler paths are compatibility surfaces, not current state authority.
- Routing and assignment selection must persist canonical events locally before deriving read models or indexes.
- Public WebSockets currently stream status, not proposal lifecycle events.

## Next Mandatory Cut

Replace polling with subscriptions:

- nodes subscribe to proposal assignments targeted to them
- nodes subscribe to active / revoked capability leases
- operator surfaces subscribe to approval requests and decisions
- the hub stops depending on `tick.py` for the proposal assignment fast path
