# Architecture

## Runtime split

Heiwa uses a narrow split between runtime and public presentation:

- **Railway** hosts the Heiwa application services: `heiwa-cloud-hq` for hub/API work and `heiwa-trading` for the trading cockpit.
- **SpacetimeDB** is the authoritative external state layer on `maincloud.spacetimedb.com`.
- **Cloudflare Pages** hosts marketing and docs for `heiwa.ltd`.
- **WebSockets** carry live status/event transport when the runtime exposes them.

## Public/runtime boundaries

- `api.heiwa.ltd` is the public HTTP + MCP ingress.
- `trade.heiwa.ltd` is the dedicated trading cockpit hostname and should route directly to the trading service.
- `status.heiwa.ltd` is a read-only status shell backed by runtime health/status data.
- `docs.heiwa.ltd` is the documentation site.
- The public web surface should not duplicate privileged runtime behavior.

## Repo boundaries

The canonical repo is `/Users/dmcgregsauce/heiwa`.

`heiwa-limited` is no longer treated as an active source-of-truth repo in this documentation set.

## State bindings

- `apps/heiwa_hub/spacetimedb/` is the Rust SpacetimeDB module.
- `packages/heiwa_bindings/rust/` and `packages/heiwa_bindings/typescript/` are generated from that module.
- Python currently uses the typed bridge in `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` until a stable generator path is adopted.
