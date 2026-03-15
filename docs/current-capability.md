# Current Capability Truth

## Supported now

- **CLI**: supported operator surface
- **MCP**: supported integration surface
- **HTTP API**: supported public-safe runtime surface
- **Docs**: supported public documentation surface

## Supported architecture claims

- Railway is the runtime host for the hub service.
- SpacetimeDB is the intended authoritative state layer.
- Cloudflare Pages hosts the public marketing and docs shell.
- Public status is WebSocket-first with HTTP fallback for diagnostics.

## Not presented as complete

- Discord as a required ingress surface
- `heiwa-limited` as an active product target
- experimental canvases as part of the supported stack
- placeholder agent personas as productized capabilities

## Evidence rule

README, MkDocs pages, and the static web shell should all agree on this boundary. CI exists to prevent public claims from drifting ahead of verified surfaces.
