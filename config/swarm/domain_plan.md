# Heiwa.ltd Domain Strategy

`heiwa.ltd` is a split-surface public topology. All domains are Cloudflare-proxied. Railway owns the live hub and serves both the API and the static web shells. SpacetimeDB stays external on `maincloud.spacetimedb.com`.

## Current live state (as of 2026-03-25)

All five public domains resolve to the same Cloudflare proxy IPs and route to the single Railway `heiwa-cloud-hq` origin. The hub serves both the API endpoints and the static HTML shells via FastAPI's `StaticFiles` mount. There is no separate Cloudflare Pages deployment yet.

| Domain | Live origin | Serves |
|--------|-------------|--------|
| `heiwa.ltd` | Railway hub via Cloudflare | Landing page (static HTML) |
| `app.heiwa.ltd` | Railway hub via Cloudflare | Product shell + dashboard (static HTML, JWT auth) |
| `api.heiwa.ltd` | Railway hub via Cloudflare | Hub API, Discord OAuth, MCP, task ingress |
| `status.heiwa.ltd` | Railway hub via Cloudflare | Status page (static HTML) |
| `docs.heiwa.ltd` | Railway hub via Cloudflare | Documentation (static HTML) |
| `trade.heiwa.ltd` | No public DNS record | Retired public hostname; standalone trading service still exists on Railway for internal preview work |

## Target state

The target topology splits static shells onto Cloudflare Pages at the edge and keeps only `api.heiwa.ltd` on Railway. This is not urgent — the current single-origin setup works — but it is the intended direction for performance and separation of concerns.

### 1. Root + marketing (`heiwa.ltd`)

- **Target host**: Cloudflare Pages
- **Current host**: Railway hub (serves static HTML)
- **Purpose**: public landing page and product positioning

### 2. Product shell (`app.heiwa.ltd`)

- **Target host**: Cloudflare Pages
- **Current host**: Railway hub (serves static HTML + JWT auth via `/auth/me`)
- **Purpose**: authenticated user shell for dashboard, key vault, run history, and mission views
- **Content**: read/write product UI that always talks back to `api.heiwa.ltd` for auth, data, and orchestration

### 3. Public status (`status.heiwa.ltd`)

- **Target host**: Cloudflare Pages
- **Current host**: Railway hub (serves static HTML)
- **Runtime source**: `api.heiwa.ltd`
- **Purpose**: read-only health and status checks
- **Transport**: WebSocket-first with HTTP fallback for diagnostics

### 4. Hub API + auth + MCP (`api.heiwa.ltd`)

- **Host**: Railway `heiwa-cloud-hq` behind Cloudflare proxy/WAF (current and target)
- **Purpose**: public-safe HTTP API, Discord OAuth entry/callback, session/user endpoints, MCP surface, runtime health, and task ingress
- **Shape**: `/auth/*`, `/health`, `/status`, `/tools`, `/call/{tool_name}`, `/tasks`, WebSocket status/events
- **Rule**: `api.heiwa.ltd` stays bound to the hub service only. Do not route vertical-specific internal services through this hostname.

### 5. Documentation (`docs.heiwa.ltd`)

- **Target host**: Cloudflare Pages
- **Current host**: Railway hub (serves static HTML)
- **Source**: MkDocs Material from the canonical repo docs
- **Purpose**: architecture, deployment, security, and operator guidance

### 6. External state ledger

- **Host**: `maincloud.spacetimedb.com` (current and target)
- **Purpose**: authoritative SpacetimeDB state ledger for hub + trading services
- **Rule**: STDB is external infrastructure. It should not be described as a Railway database, sidecar, or attached volume.

## Internal or preview-only, not first-class public

- `trade.heiwa.ltd` no longer has a public DNS record. The standalone `heiwa-trading` Railway service still exists for internal preview work, but it is not part of the supported public surface and should not appear in the public host manifest or marketing story.
- Other vertical-specific runtimes can remain separate Railway services as long as `app.heiwa.ltd` stays the canonical user home and `api.heiwa.ltd` stays the single public control plane.

## Planned, not first-class

- `auth.heiwa.ltd` may exist later, but it is not part of the supported v1 surface.
- Discord is not treated as a required public entry point in the domain plan.

## Next steps

1. Remove public DNS for `trade.heiwa.ltd` (done 2026-03-25).
2. Keep `api.heiwa.ltd` on Railway `heiwa-cloud-hq`.
3. Keep SpacetimeDB external on `maincloud.spacetimedb.com`.
4. Optionally migrate `heiwa.ltd`, `app.heiwa.ltd`, `status.heiwa.ltd`, `docs.heiwa.ltd` to Cloudflare Pages when edge performance or ops isolation justifies it.
5. Keep internal vertical runtimes off the supported public host map unless they graduate into first-class product surfaces.
6. Prefer WebSocket-backed public status/event views over poll-heavy status pages.
