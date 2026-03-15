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
- `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- `packages/heiwa_sdk/heiwa_sdk/db.py`
- `packages/heiwa_protocol/heiwa_protocol/routing.py`
- `apps/heiwa_hub/mcp_server.py`

## Design Rules

- Prefer typed contracts over implicit dicts.
- Prefer STDB-backed service layers over raw SQL in public runtime paths.
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
