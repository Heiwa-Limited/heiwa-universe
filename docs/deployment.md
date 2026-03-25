# Deployment

## Railway

Railway is the cloud runtime for the live Heiwa services.

Expected public runtime surfaces:

- `api.heiwa.ltd` -> `heiwa-cloud-hq`
- Hub endpoints: `/auth/*`, `/health`, `/status`, `/tools`, `/call/{tool_name}`, WebSocket status/events
- Internal vertical services such as `heiwa-trading` can still deploy on Railway, but they are not treated as supported public runtime surfaces

## Cloudflare

Cloudflare proxies all public domains. Currently all domains route to the Railway hub, which serves both the API and the static web shells. The target state splits the static shells onto Cloudflare Pages at the edge, but this migration is not yet done.

Target Cloudflare Pages surfaces (not yet split from Railway):

- root marketing pages (`heiwa.ltd`)
- authenticated app shell (`app.heiwa.ltd`)
- status shell (`status.heiwa.ltd`)
- docs site built from MkDocs Material (`docs.heiwa.ltd`)

`trade.heiwa.ltd` no longer has a public DNS record. The standalone `heiwa-trading` Railway service still exists for internal preview work, but it is not a supported public surface.

Cloudflare should present public-safe shells. It should not become a second control plane or make privileged decisions outside the hub API.

## SpacetimeDB

SpacetimeDB remains external infrastructure on `maincloud.spacetimedb.com`.

- Do not describe it as a Railway sidecar, attached volume, or private database service.
- Hub and any internal vertical runtimes should both treat it as the shared authoritative ledger.

## Build outputs

- `apps/heiwa_web/clients/web`: static marketing, app shell, and status shell
- `mkdocs build`: documentation output for `docs.heiwa.ltd`

## Verification

- hub smoke tests must run in CI
- internal trading deploys stay optional and are not verified through a public hostname gate
- docs must build with `mkdocs build --strict`
- the static web shell must pass `python apps/heiwa_web/scripts/check_static_surface.py`
