# Architecture

## Runtime split

Heiwa uses a narrow split between runtime and public presentation:

- **Railway** hosts the Heiwa application services: `heiwa-cloud-hq` for hub/API work and optional internal runtimes such as `heiwa-trading`.
- **SpacetimeDB** is the authoritative external state layer on `maincloud.spacetimedb.com`.
- **Cloudflare** proxies all public domains. Currently all route to Railway, which serves both API and static shells. The target state splits the static shells onto Cloudflare Pages at the edge.
- **WebSockets** carry live status/event transport when the runtime exposes them.

## Public/runtime boundaries

- `heiwa.ltd` is the public marketing hostname.
- `app.heiwa.ltd` is the canonical authenticated product shell.
- `api.heiwa.ltd` is the public HTTP + MCP ingress.
- `status.heiwa.ltd` is a read-only status shell backed by runtime health/status data.
- `docs.heiwa.ltd` is the documentation site.
- Internal vertical runtimes such as trading can stay on separate Railway services, but they are not part of the supported public surface until they graduate into first-class product surfaces.
- The public web surface should not duplicate privileged runtime behavior.

## Repo boundaries

The canonical repo is `/Users/dmcgregsauce/heiwa`.

`heiwa-limited` is no longer treated as an active source-of-truth repo in this documentation set.

## State bindings

- `apps/heiwa_hub/spacetimedb/` is the Rust SpacetimeDB module.
- `packages/heiwa_bindings/rust/` and `packages/heiwa_bindings/typescript/` are generated from that module.
- Python currently uses the typed bridge in `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` until a stable generator path is adopted.
