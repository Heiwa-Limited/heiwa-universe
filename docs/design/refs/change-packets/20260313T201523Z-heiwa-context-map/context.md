# Figma Sync Context

- Profile: `heiwa-one-system`
- Repo: `Strategizing/heiwa-universe`
- Repo root: `/Users/dmcgregsauce/heiwa`
- Sync mode: `manual_packet`
- Generated (UTC): `2026-03-13T20:15:23Z`

## Intent

Update the Heiwa architecture visuals to reflect that the repo now has a canonical cold-start context map:

- `HEIWA.md` is the root routing table for agents and operators
- `SOUL.md` is now a real compatibility shim at repo root
- `rooms/` is now the explicit context silo layer for control-plane, execution, orchestration, infra, and SDK work

## Constraint Note

This is a repo-readable operator/agent UI layer, not a replacement for the runtime topology.

Keep the runtime path visible:

- Broker
- HeiwaClaw
- MCP / adapters
- SpacetimeDB
- WebSockets

Show the context map as the repo-native guidance layer that sits above those runtime components.
