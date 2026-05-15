# Architecture

## Product Stack

Heiwa uses a narrow split between local execution, public presentation, and canonical state:

- **Local runtime**: installed `heiwa` owns provider subprocesses, local model calls, shell/browser/computer-use side effects, local secrets, and local approvals.
- **SpacetimeDB Maincloud**: authoritative state layer on `maincloud.spacetimedb.com`; owns reducers, subscriptions, leases, runs, artifacts, and evidence.
- **Cloudflare**: public edge for marketing, docs, status, static clients, WAF, DNS, and later Workers/remote attach.
- **GitHub**: source, CI, release artifacts, installer distribution, and professional public repo front page.

The target enterprise backbone is GitHub + Cloudflare + SpacetimeDB. The local runtime still owns the hot path for provider streams, shell work, local models, approvals, and side effects; hosted availability does not mean every user action runs remotely.

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
- Internal vertical runtimes such as trading are not part of the supported public surface until they graduate into first-class product surfaces.
- The public web surface should not duplicate privileged runtime behavior.

## Repo boundaries

The canonical active repo is `/Users/dmcgregsauce/heiwa-universe`.

`heiwa-limited` is no longer treated as an active source-of-truth repo in this documentation set.

## State bindings

- Current STDB-facing Rust work lives in `apps/heiwa_core/src/stdb/`, `apps/heiwa_orchestrator/src/stdb/`, and `crates/heiwa_stdb/`.
- `legacy/apps/heiwa_hub/spacetimedb/` is quarantined migration/reference material. Do not treat it as the active product spine.
- `packages/heiwa_bindings/rust/` and `packages/heiwa_bindings/typescript/` are generated from that module.
- Python currently uses the typed bridge in `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` until a stable generator path is adopted.
