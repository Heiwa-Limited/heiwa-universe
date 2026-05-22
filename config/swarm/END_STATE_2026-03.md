# Heiwa End-State: Objective Conceptualization

Status: updated 2026-05-22. This file describes direction, not current parity.

## What Heiwa Is

Heiwa is a local-first operating layer that preserves identity, state, memory,
governance, routing, and execution continuity across changing models, tools,
and user-owned nodes.

## Production Stack Direction

### Rust

Rust is the target backend language for local runtime behavior, routing, DREX
scoring, persistence of decisions/failures/leases, and STDB-facing reducers.

### TypeScript

TypeScript owns the cockpit and future public/remote operator surfaces.

### Shell

Shell remains first-class for local bootstrap, Linux/WSL execution surfaces, CLI
wrappers, auth materialization, and runtime glue.

### Python

Python remains compatibility, sidecar, and regression support until specific
modules are promoted behind Rust-owned contracts.

## Deployment Model

- **MacBook owner runtime:** current source-of-truth/server for user functionality.
- **SpacetimeDB:** evidence sync/adjudication plane when enabled.
- **Cloudflare:** paused public edge and static shell/docs host when re-enabled.
- **WSL/GPU nodes:** optional sovereign/local execution capacity.

## Authoritative State Model

Current user functionality is local-first under `~/.heiwa` and repo-owned files.
STDB should mirror/adjudicate missions, proposals, route decisions, leases,
approvals, events, and memory projections when online.

## Execution Flow

```text
Input (CLI / local cockpit / future webhook)
  -> Shell bootstrap (env, auth, local state readiness)
  -> Rust runtime
  -> DREX scoring + routing
  -> Local state write, optional STDB sync
  -> Execution lane selection (MacBook, WSL, provider CLI, provider API)
  -> Result / state transition / memory projection
  -> TypeScript cockpit + future edge views
```

## Success Criteria

- Rust owns the local orchestration and routing path.
- TypeScript owns the primary operator-facing application.
- Shell remains stable bootstrap glue.
- Python is explicitly sidecar/reference unless promoted.
- Local user functionality works without public access.
- Public edge can be re-enabled later without replacing the local runtime truth.
