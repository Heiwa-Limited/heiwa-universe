# Heiwa End-State: Objective Conceptualization (April 2026 Update)

> **Status**: Target end-state, not current operational reality. The repo still contains Python-first runtime paths, static web surfaces, and transitional glue. This document defines the direction the codebase should be converging toward without overstating what already ships.

## What Heiwa Is

Heiwa is a frontier digital enterprise system: an always-on Agentic Digital Entity that preserves identity, state, memory, governance, and execution continuity across changing models, tools, and nodes.

It should be treated as a computationally legible organization layer, not a chatbot, not a thin multi-agent wrapper, and not a Python application with a few AI hooks attached.

## Production Stack Direction

### Rust (Primary Control Plane)

Rust is the target backend language for:

- SpacetimeDB reducers, tables, and invariant-preserving state logic
- orchestration and runtime supervision
- routing and DREX scoring
- persistence of decisions, failures, leases, and execution telemetry

### TypeScript (Primary Operator Surface)

TypeScript is the target language for:

- operator-facing web applications
- generated client contracts over SpacetimeDB
- dashboards, routing views, and administrative control surfaces
- protocol mirrors that need to stay type-aligned with Rust state

### Shell (Bootstrap and Execution Glue)

Shell remains first-class for:

- Railway bootstrapping
- Linux and WSL execution surfaces
- CLI wrappers and operator workflows
- environment setup, auth materialization, and runtime glue

### Python (Legacy and Regression Only)

Python remains in the repo for:

- compatibility during migration
- regression tests against the old Hub/cognition paths
- bridge code that has not been ported yet

Python is **not** the target production control plane.

## Deployment Model

### Railway (Primary Runtime Plane)

Railway stays the always-on runtime plane. Its job is to keep the entity reachable, supervised, and stateful even when boost nodes are offline.

Railway should host:

- the Rust orchestrator
- the SpacetimeDB-backed control plane
- CLI and HTTP execution surfaces that remain cloud-safe
- shell bootstrap scripts that prepare the environment before handing off to Rust

### Cloudflare (Edge and Public Surfaces)

Cloudflare remains the edge boundary and public presentation layer.

Cloudflare should host:

- public and semi-public operator surfaces
- ingress-safe edge reductions
- status and governance views that expose already-reduced state instead of raw runtime internals

### Boost Nodes (Optional Capacity)

MacBook and WSL nodes remain optional.

When online, they contribute:

- local filesystem access
- GPU and local inference capacity
- Docker and local build surfaces
- trusted local execution for sovereign tasks

When offline, the primary plane should still function.

## Authoritative State Model

SpacetimeDB remains the source of truth for:

- missions
- proposals
- route decisions
- DREX decisions and failures
- capability leases
- node and executor state
- approvals
- events
- memory projections and summaries

The system should move toward a model where state transitions are typed, queryable, and reducer-backed instead of reconstructed from logs or hidden in process memory.

## Execution Flow

```
Input (CLI / Discord / Webhook / Cron)
  → Shell bootstrap (env, auth, STDB readiness, optional local services)
  → Rust orchestrator
  → DREX scoring + routing
  → STDB-backed decision persistence
  → Execution lane selection (Railway surface or boost node)
  → Result / state transition / memory projection
  → TypeScript operator surfaces + edge views
```

## Resolution-Native Intelligence

Heiwa should route work according to **DREX**: Dynamic Resolution of Execution.

- **Macro**: strategic planning, policy, coordination, resource posture
- **Meso**: domain routing, supervision, lease shaping, escalation
- **Micro**: shell, files, APIs, runtime mutation, tactical execution

The enterprise view should be a structured reduction of lower-level activity, not a separate narrative system.

## What Gets Replaced

| Legacy Primary Path | Target Replacement |
| --- | --- |
| Python Hub as primary control plane | Rust orchestrator + Rust-owned routing |
| Python cognition router as the long-term runtime selector | Rust DREX scoring and route selection |
| Static HTML web as the main operator surface | TypeScript operator application |
| Ad hoc JSON contracts between layers | Generated, typed Rust and TypeScript bindings |
| Python-first mental model of the repo | Rust + TypeScript + Shell-first architecture |

## What Stays

| Surface | Ongoing Role |
| --- | --- |
| SpacetimeDB | Authoritative durable state |
| Railway | Primary always-on runtime plane |
| Cloudflare | Edge ingress and public presentation |
| Shell | Bootstrap, execution glue, operator workflows |
| Boost nodes | Optional sovereign/local execution capacity |

## Success Criteria

- Rust owns the production orchestration and routing path
- TypeScript owns the primary operator-facing web application
- Shell remains stable bootstrap glue without becoming the cognitive runtime
- Python is demoted to compatibility and regression-only status
- SpacetimeDB remains authoritative throughout the migration
- The system can operate continuously on Railway while still exploiting local boost nodes when available
