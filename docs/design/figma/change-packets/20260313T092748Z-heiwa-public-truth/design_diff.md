# Design Drift Summary

- Retire any visual that treats `heiwa-limited` as an active target or canonical repo.
- Reduce the public surface to four supported entries: CLI, MCP, HTTP API, docs.
- Move marketing/docs/status shell to Cloudflare Pages in the diagram.
- Keep Railway as the runtime ingress only, not the docs/marketing host.
- Show SpacetimeDB as the authoritative state direction.
- Show WebSocket-first status/event transport, with HTTP fallback only as diagnostics.
- Remove or visually de-emphasize Discord, auth, experimental canvas, and placeholder agent personas from any “stack-complete” framing.
- Reflect CI as the enforcement point for hub smoke tests, docs build, and static public-shell checks.
