# Infra Room

Load this room for:

- Cloudflare changes
- CI / deployment work
- runtime topology validation
- build and verification workflows

## Runtime Topology

- The MacBook checkout plus `~/.heiwa/` host current user functionality.
- SpacetimeDB is the optional evidence sync/adjudication plane.
- Cloudflare serves docs / marketing shell only after public access is re-enabled.
- WebSockets are the live transport for local cockpit status and future event subscriptions.

## Important Files

- `.github/workflows/deploy.yml`
- `apps/heiwa_core/`
- `apps/heiwa_orchestrator/`
- `crates/heiwa_stdb/`
- `legacy/apps/heiwa_hub/main.py` when repairing/promoting legacy Hub only
- `legacy/apps/heiwa_hub/mcp_server.py` when repairing/promoting legacy Hub only
- `legacy/apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh` when regenerating legacy bindings only
- `packages/heiwa_bindings/*`

## Current CI Expectations

- hub smoke tests
- agent context map test
- HeiwaCells catalog test
- HeiwaBench release-gate test
- docs build
- static web checks
- repo hygiene

## Infra Rules

- Do not add polling-oriented public surfaces where subscriptions or WebSockets belong.
- Do not route durable control-plane state through external message brokers.
- Do not make Cloudflare or Discord look like the authority for runtime truth.
