# Infra Room

Load this room for:

- Railway and Cloudflare changes
- CI / deployment work
- runtime topology validation
- build and verification workflows

## Runtime Topology

- Railway hosts the hub runtime and always-on control plane
- SpacetimeDB is the state plane
- Cloudflare serves docs / marketing shell
- WebSockets are the live transport for public status and future event subscriptions

## Important Files

- `.github/workflows/deploy.yml`
- `apps/heiwa_hub/main.py`
- `apps/heiwa_hub/mcp_server.py`
- `apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh`
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
- Do not route durable control-plane state through NATS.
- Do not make Cloudflare or Discord look like the authority for runtime truth.
