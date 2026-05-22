# Architecture

## Product Stack

Heiwa uses a narrow split between installed execution, backend state, hosted support, and public presentation:

- **Installed `heiwa` runtime** is the primary operator surface. It owns local routing, cockpit, bounded execution, provider wrapping, and operator UX.
- **Rust workspace services** own the current execution kernel, DREX routing, provider/session/protocol crates, connector gates, and release artifacts.
- **SpacetimeDB** is the backend adjudication, subscription, and evidence plane on `maincloud.spacetimedb.com`.
- **Owner-managed local and approved hosted support** may run specific Heiwa services where always-on infrastructure is needed, but support hosts are not the product center.
- **GitHub and Cloudflare edge infrastructure** publish docs, releases, CI evidence, and public shells. Public surfaces should not duplicate privileged runtime behavior.
- **WebSockets** carry live status/event transport when a runtime exposes them.

## Capability Fabric

External accounts, tools, devices, models, and agents enter Heiwa through typed
capability lanes. See [`capability-fabric.md`](capability-fabric.md) for the
connector contract, subagent delegation model, and value gate.

## Public/runtime boundaries

- `heiwa.ltd` is the public marketing hostname.
- `app.heiwa.ltd` is the safe companion client shell when authenticated and policy-backed.
- `api.heiwa.ltd` is the public HTTP/MCP/status ingress where deployed.
- `status.heiwa.ltd` is a read-only status shell backed by runtime health/status data.
- `docs.heiwa.ltd` is the documentation site.
- Internal vertical runtimes such as trading can stay isolated from the supported public surface until they graduate into first-class product surfaces.
- The public web surface should not duplicate privileged runtime behavior.

## Repo boundaries

The canonical active repo is `/Users/dmcgregsauce/heiwa-universe`.

`heiwa-limited` is no longer treated as an active source-of-truth repo in this documentation set.

## State bindings

- Current STDB-facing Rust work lives in `apps/heiwa_core/src/stdb/`, `apps/heiwa_orchestrator/src/stdb/`, and `crates/heiwa_stdb/`.
- `legacy/apps/heiwa_hub/spacetimedb/` is quarantined migration/reference material. Do not treat it as the active product spine.
- `packages/heiwa_bindings/rust/` and `packages/heiwa_bindings/typescript/` are generated bindings for STDB-facing contracts.
- Python currently uses the typed bridge in `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` until a stable generator path is adopted.
