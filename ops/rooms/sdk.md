# SDK Room

Load this room for:

- `heiwa_sdk` changes
- MCP / HTTP public surface changes
- protocol contract updates
- agent-readable API design

## Public Surface Anchors

- `packages/heiwa_sdk/heiwa_sdk/heiwaclaw.py`
- `packages/heiwa_sdk/heiwa_sdk/bench.py`
- `packages/heiwa_sdk/heiwa_sdk/cells.py`
- `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` — LEGACY. STDB was extracted from Rust
  on 2026-07-15 but this Python module is still tracked and still imported by
  `heiwa_cli/commands.py`, `agent_memory.py`, `db.py`, and `heiwaclaw/gateway.py`.
  Retiring it is open work; do not build new surfaces on it.
- `packages/heiwa_sdk/heiwa_sdk/db.py`
- `packages/heiwa_protocol/heiwa_protocol/routing.py`

## Design Rules

- Prefer typed contracts over implicit dicts.
- Prefer evidence-backed service layers (`crates/heiwa_evidence`) over raw SQL in
  public runtime paths. Local SQLite stays legitimate for hot state.
- Keep public surfaces honest: if it is exposed via CLI, MCP, HTTP, or docs, it needs tests.
- New public behavior should usually gain HeiwaBench coverage or a direct smoke test.

## Current Live Seed Surfaces

- `HeiwaCells`:
  - CLI: `heiwa cells`
  - MCP: `heiwa_get_cells_catalog`
- `HeiwaBench`:
  - CLI: `heiwa bench`
  - MCP: `heiwa_run_bench`

These are intentionally seed-stage. Expand them without breaking their contract surface.
