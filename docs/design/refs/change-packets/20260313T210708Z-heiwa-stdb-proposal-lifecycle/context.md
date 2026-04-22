# Figma Sync Context

- Profile: `heiwa-one-system`
- Repo: `Strategizing/heiwa-universe`
- Repo root: `/Users/dmcgregsauce/heiwa`
- Sync mode: `manual_packet`
- Generated (UTC): `2026-03-13T21:07:08Z`

## Intent

Update the Heiwa architecture visuals to reflect that proposal authority has materially moved onto SpacetimeDB:

- STDB now has first-class tables for `proposals`, `proposal_consents`, `approval_requests`, `approval_decisions`, and `capability_leases`
- the Python bridge and `db.py` now route proposal creation, assignment, claim, consent, heartbeat, and lease issuance through STDB fast paths
- the FastAPI proposal surface keeps the same public endpoints, but those endpoints now write through the STDB path when `HEIWA_STATE_BACKEND=spacetimedb`

## Constraint Note

This is not the final subscription-native control plane yet.

Keep these truths explicit:

- STDB is now the authority for proposal / approval / lease state
- `tick.py` still exists as the routing scheduler that selects nodes and writes assignments into STDB
- public WebSockets still stream status, not proposal lifecycle events
- `approval.py` still exists as a compatibility in-memory registry and should be shown as transitional, not authoritative
