# Heiwa.ltd Domain Strategy

Status: local-first reset, 2026-05-22.

Public access is paused. The MacBook checkout plus `~/.heiwa/` are the current
source-of-truth/server for user functionality. Cloudflare remains future edge
infrastructure; it must not be treated as runtime authority.

## Current Live State

| Surface | Current host | Purpose |
| --- | --- | --- |
| `heiwa app start` | `127.0.0.1:7474` on Devon's MacBook | Cockpit, local API, runtime status |
| `~/.heiwa/` | Devon's MacBook | Local identity, state, sessions, approvals, workers |
| `maincloud.spacetimedb.com` | Optional external service | Evidence sync/adjudication when enabled |
| `heiwa.ltd` / `app.heiwa.ltd` / `api.heiwa.ltd` | Paused | No public user access target yet |

## Target State

1. Finish the local owner runtime first.
2. Keep the cockpit local until auth, routing, hook posture, and local state are reliable.
3. Re-enable Cloudflare only as public edge for static shell/docs and a future API target.
4. Keep SpacetimeDB external; do not model it as an app-host volume or sidecar.

## Domain Rules

- Do not point public DNS at stale origins.
- Do not list `auth.heiwa.ltd` or `trade.heiwa.ltd` as active public surfaces.
- Do not make Cloudflare the authority for runtime truth.
- Public DNS records stay disabled in Terraform unless `enable_public_dns=true`
  and explicit non-empty CNAME targets are provided.

## Next Steps

1. Verify `heiwa app start` and `heiwa app runtime status --json` locally.
2. Build the cockpit against localhost API endpoints.
3. Keep Cloudflare credentials absent or read-only until public access is explicitly re-enabled.
4. When ready, publish static docs/shell first; expose API only after local auth and state gates pass.
