# Deployment

## Railway

Railway is the cloud runtime for the hub service.

Expected public runtime surfaces:

- `/health`
- `/status`
- `/tools`
- `/call/{tool_name}`
- WebSocket status/events endpoint

## Cloudflare

Cloudflare Pages is the public shell for:

- root marketing pages
- status shell
- docs site built from MkDocs Material

Cloudflare should present public-safe, read-only views. It should not become a second control plane.

## Build outputs

- `apps/heiwa_web/clients/web`: static marketing and status shell
- `mkdocs build`: documentation output for `docs.heiwa.ltd`

## Verification

- hub smoke tests must run in CI
- docs must build with `mkdocs build --strict`
- the static web shell must pass `python apps/heiwa_web/scripts/check_static_surface.py`
