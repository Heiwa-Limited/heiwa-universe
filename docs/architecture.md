# Architecture

## Product Stack

Heiwa uses a narrow split between installed execution, local durable state, and repository-backed distribution:

- **Installed `heiwa` runtime** is the primary execution surface. It owns local routing, bounded execution, provider wrapping, and operator UX.
- **Installed Heiwa.app** is the primary user input/display surface. It lives under the user's HOME-local Heiwa root and renders the runtime state without becoming a second authority.
- **Rust workspace services** own the current execution kernel, DREX routing, provider/session/protocol crates, connector gates, and release artifacts.
- **Local JSONL journals** are canonical evidence truth; **Lance** is a derived, rebuildable local recall index.
- **Owner-managed local and approved hosted support** may run specific Heiwa services where always-on infrastructure is needed, but support hosts are not the product center.
- **GitHub** publishes source, docs, releases, and CI evidence. **Cloudflare** is DNS utility only.
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
- The localhost/browser console is a user-scoped pseudo-backend for advanced
  settings, telemetry, connectors, and support views. It is not the normal
  primary UI.
- Internal vertical runtimes such as trading can stay isolated from the supported public surface until they graduate into first-class product surfaces.
- The public web surface should not duplicate privileged runtime behavior.

## Repo boundaries

The canonical active repo is `/Users/dmcgregsauce/heiwa-universe`.

`heiwa-limited` is no longer treated as an active source-of-truth repo in this documentation set.

## State bindings

- `crates/heiwa_evidence/` owns versioned JSONL journals, replay, compaction, and recovery.
- `crates/heiwa_embed/` owns SQLite/Lance vector backends; Lance data is derived and rebuildable.
- Python SDK adapters that mention the retired backend are compatibility-only and are not runtime authority.
