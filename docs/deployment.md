# Deployment

## Railway

Railway is the cloud runtime for the live Heiwa services.

Expected public runtime surfaces:

- `api.heiwa.ltd` -> `heiwa-cloud-hq`
- `trade.heiwa.ltd` -> `heiwa-trading`
- Hub endpoints: `/health`, `/status`, `/tools`, `/call/{tool_name}`, WebSocket status/events
- Trading cockpit: dedicated service-owned root surface and trading routes; the hub does not serve `/trading/*`

## Cloudflare

Cloudflare Pages is the public shell for:

- root marketing pages
- status shell
- docs site built from MkDocs Material

Cloudflare should present public-safe, read-only views. It should not become a second control plane.

## SpacetimeDB

SpacetimeDB remains external infrastructure on `maincloud.spacetimedb.com`.

- Do not describe it as a Railway sidecar, attached volume, or private database service.
- Hub and trading should both treat it as the shared authoritative ledger.

## Build outputs

- `apps/heiwa_web/clients/web`: static marketing and status shell
- `mkdocs build`: documentation output for `docs.heiwa.ltd`

## Verification

- hub smoke tests must run in CI
- trading service checks must verify the dedicated `trade.heiwa.ltd` route shape
- docs must build with `mkdocs build --strict`
- the static web shell must pass `python apps/heiwa_web/scripts/check_static_surface.py`
