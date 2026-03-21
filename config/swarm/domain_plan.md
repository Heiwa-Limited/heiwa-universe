# Heiwa.ltd Domain Strategy

`heiwa.ltd` is a split-service public topology. Cloudflare Pages owns the public shells, Railway owns the live application services, and SpacetimeDB stays external on `maincloud.spacetimedb.com` rather than being modeled as a Railway volume/service.

## 1. Root + marketing (`heiwa.ltd`)

- **Host**: Cloudflare Pages
- **Purpose**: public landing page and product positioning
- **Content**: supported surfaces, hosting model, and public-safe architecture summary

## 2. Public status (`status.heiwa.ltd`)

- **Host**: Cloudflare Pages
- **Runtime source**: `api.heiwa.ltd`
- **Purpose**: read-only health and status checks
- **Transport**: WebSocket-first with HTTP fallback for diagnostics

## 3. Hub API + MCP (`api.heiwa.ltd`)

- **Host**: Railway `heiwa-cloud-hq` behind Cloudflare proxy/WAF
- **Purpose**: public-safe HTTP API, MCP surface, runtime health, and task ingress
- **Shape**: `/health`, `/status`, `/tools`, `/call/{tool_name}`, `/tasks`, WebSocket status/events
- **Rule**: `api.heiwa.ltd` stays bound to the hub service only. Do not route trading UI traffic through this hostname.

## 4. Trading cockpit (`trade.heiwa.ltd`)

- **Host**: Railway `heiwa-trading` behind Cloudflare proxy/WAF
- **Purpose**: dedicated trading cockpit and service-specific workflows
- **Shape**: trading landing surface at `/` with service-owned routes behind the dedicated hostname
- **Rule**: route traffic directly to `heiwa-trading`; do not multiplex it through `heiwa-cloud-hq`.

## 5. Documentation (`docs.heiwa.ltd`)

- **Host**: Cloudflare Pages
- **Source**: MkDocs Material from the canonical repo docs
- **Purpose**: architecture, deployment, security, and operator guidance

## 6. External state ledger

- **Host**: `maincloud.spacetimedb.com`
- **Purpose**: authoritative SpacetimeDB state ledger for hub + trading services
- **Rule**: STDB is external infrastructure. It should not be described as a Railway database, sidecar, or attached volume.

## Planned, not first-class

- `auth.heiwa.ltd` may exist later, but it is not part of the supported v1 surface.
- Discord is not treated as a required public entry point in the domain plan.

## Next steps

1. Keep `heiwa.ltd`, `status.heiwa.ltd`, and `docs.heiwa.ltd` on Cloudflare Pages.
2. Keep `api.heiwa.ltd` on Railway `heiwa-cloud-hq`.
3. Route `trade.heiwa.ltd` directly to Railway `heiwa-trading`.
4. Keep SpacetimeDB external on `maincloud.spacetimedb.com`.
5. Prefer WebSocket-backed public status/event views over poll-heavy status pages.
