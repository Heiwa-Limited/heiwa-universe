# Figma Sync Context

- Profile: `heiwa-one-system`
- Repo: `Strategizing/heiwa-universe`
- Repo root: `/Users/dmcgregsauce/heiwa`
- Sync mode: `manual_packet`
- Generated (UTC): `2026-03-13T09:43:17Z`

## Intent

Update the architecture/state visuals so they reflect the new fast-state pass:

- `apps/heiwa_hub/spacetimedb` is now the canonical Rust module for route/run/node/liveness state.
- Broker route decisions, telemetry run records, node heartbeats, and liveness state now have explicit SpacetimeDB reducers/tables.
- `packages/heiwa_bindings/rust` and `packages/heiwa_bindings/typescript` are generated from the live module.
- Python uses the typed bridge in `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`.
- WebSocket status remains the public live transport.

## Constraint Note

The proposal/lease/RFC workflow is still partly compatibility-SQL shaped. Do not depict that lane as fully migrated to SpacetimeDB yet.
