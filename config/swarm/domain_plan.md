# Heiwa.ltd Domain Strategy

`heiwa.ltd` is a split-surface public topology. Cloudflare Pages owns the public shells, Railway owns the live hub API, and SpacetimeDB stays external on `maincloud.spacetimedb.com` rather than being modeled as a Railway volume/service.

## 1. Root + marketing (`heiwa.ltd`)

- **Host**: Cloudflare Pages
- **Purpose**: public landing page and product positioning
- **Content**: supported surfaces, hosting model, and public-safe architecture summary

## 2. Product shell (`app.heiwa.ltd`)

- **Host**: Cloudflare Pages
- **Purpose**: authenticated user shell for dashboard, key vault, run history, and mission views
- **Content**: read/write product UI that always talks back to `api.heiwa.ltd` for auth, data, and orchestration

## 3. Public status (`status.heiwa.ltd`)

- **Host**: Cloudflare Pages
- **Runtime source**: `api.heiwa.ltd`
- **Purpose**: read-only health and status checks
- **Transport**: WebSocket-first with HTTP fallback for diagnostics

## 4. Hub API + auth + MCP (`api.heiwa.ltd`)

- **Host**: Railway `heiwa-cloud-hq` behind Cloudflare proxy/WAF
- **Purpose**: public-safe HTTP API, Discord OAuth entry/callback, session/user endpoints, MCP surface, runtime health, and task ingress
- **Shape**: `/auth/*`, `/health`, `/status`, `/tools`, `/call/{tool_name}`, `/tasks`, WebSocket status/events
- **Rule**: `api.heiwa.ltd` stays bound to the hub service only. Do not route vertical-specific internal services through this hostname.

## 5. Documentation (`docs.heiwa.ltd`)

- **Host**: Cloudflare Pages
- **Source**: MkDocs Material from the canonical repo docs
- **Purpose**: architecture, deployment, security, and operator guidance

## 6. External state ledger

- **Host**: `maincloud.spacetimedb.com`
- **Purpose**: authoritative SpacetimeDB state ledger for hub + trading services
- **Rule**: STDB is external infrastructure. It should not be described as a Railway database, sidecar, or attached volume.

## Internal or preview-only, not first-class public

- `trade.heiwa.ltd` may stay attached to Railway `heiwa-trading` for internal preview work, but it is not part of the supported public surface and should not appear in the public host manifest or marketing story.
- Other vertical-specific runtimes can remain separate Railway services as long as `app.heiwa.ltd` stays the canonical user home and `api.heiwa.ltd` stays the single public control plane.

## Planned, not first-class

- `auth.heiwa.ltd` may exist later, but it is not part of the supported v1 surface.
- Discord is not treated as a required public entry point in the domain plan.

## Next steps

1. Keep `heiwa.ltd`, `app.heiwa.ltd`, `status.heiwa.ltd`, and `docs.heiwa.ltd` on Cloudflare Pages.
2. Keep `api.heiwa.ltd` on Railway `heiwa-cloud-hq`.
3. Keep SpacetimeDB external on `maincloud.spacetimedb.com`.
4. Keep internal vertical runtimes off the supported public host map unless they graduate into first-class product surfaces.
5. Prefer WebSocket-backed public status/event views over poll-heavy status pages.
