# Heiwa

Heiwa is the canonical control-plane repo for the Heiwa stack.

## Supported surfaces

- CLI
- MCP
- HTTP API
- Docs

Discord, mobile canvases, and legacy portability scaffolding may still exist in the repo, but they are not treated as stack-complete public surfaces.

## Current target architecture

- **Runtime**: Railway
- **State**: SpacetimeDB
- **Public web/docs**: Cloudflare Pages
- **Live transport**: WebSockets for status and event streaming
- **Operator node**: MacBook M4 Pro 24GB

## Design intent

Heiwa is being hardened toward a faster stack:

- retire slow compatibility paths
- remove inflated public claims
- route public status through WebSocket-first surfaces
- keep the canonical repo at `/Users/dmcgregsauce/heiwa`
- retire `heiwa-limited` as a public-facing target description

## Truth boundary

If a surface is not covered by the current docs, build checks, or hub smoke tests, it should not be presented here as complete.

## Source of truth

- [`config/swarm/BUILD_BLUEPRINT_2026-03-06.md`](https://github.com/Strategizing/heiwa-universe/blob/main/config/swarm/BUILD_BLUEPRINT_2026-03-06.md)
- [`config/swarm/ai_router.json`](https://github.com/Strategizing/heiwa-universe/blob/main/config/swarm/ai_router.json)
- [`config/identities/profiles.json`](https://github.com/Strategizing/heiwa-universe/blob/main/config/identities/profiles.json)
- [`config/swarm/domain_plan.md`](https://github.com/Strategizing/heiwa-universe/blob/main/config/swarm/domain_plan.md)
