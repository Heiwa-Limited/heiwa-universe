# Acceptance Checklist

- [ ] SpacetimeDB is shown as the authoritative state plane for route/run/node/liveness state.
- [ ] The Rust module at `apps/heiwa_hub/spacetimedb` is visible in the architecture.
- [ ] The Python bridge is shown as an adapter, not the source of truth.
- [ ] Generated Rust and TypeScript bindings are shown as outputs of the module.
- [ ] WebSocket public status is still shown as the preferred live transport.
- [ ] The proposal/lease/RFC lane is clearly marked as a remaining compatibility boundary.
- [ ] No visual implies that all SQL-era state has already been removed.
