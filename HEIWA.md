# HEIWA.md

Heiwa is a distributed AI operating system with one logical identity and multiple physical homes.

## Read Order

Read these in order before making runtime or architecture changes:

1. `HEIWA.md`
2. `AGENTS.md`
3. `SOUL.md`
4. `config/swarm/BUILD_BLUEPRINT_2026-03-06.md`
5. `config/swarm/END_STATE_2026-03.md` — target architecture and kill list
6. `config/swarm/ai_router.json`
7. `config/identities/profiles.json`
8. The room files relevant to the task

## Architecture Of Record

- **Railway (Linux)**: The always-on Primary Plane and ultimate agent harness.
- **MacBook (macOS)**: Staging environment and high-trust orchestrator.
- **SpacetimeDB**: The authoritative state ledger (Sovereignty).
- **Consolidated Monorepo**: `~/heiwa` is the *only* active workspace. All ghost repositories have been archived.

## Harness Scaffolding (Ground Truth)

The system is anchored by machine-readable files that persist across agent sessions:
- `docs/superpowers/status/feature_list.json`: The inviolable truth of system capabilities.
- `docs/superpowers/status/progress.md`: The multi-session handoff log.
- `scripts/init_env.sh`: Standardized environment initialization for all nodes.

## Hard Rules

- **State Sovereignty**: Write important state to SpacetimeDB first. SQLite is retired.
- **Harness Fidelity**: Always develop for Railway/Linux target. The MacBook is for staging.
- **Transport**: Prefer subscriptions and WebSockets over polling.
- **Execution**: Route model and tool execution through `HeiwaClaw` / MCP.
- **Economy**: Cheapest acceptable route first.
- **Privacy**: Sovereign work stays on local boost nodes.

## Transitional Boundaries

- Proposal / lease / RFC flow is still partly compatibility-SQL and polling-shaped.
- `HeiwaCells` is a real catalog, but not yet a full installer/marketplace surface.
- `HeiwaBench` is a real release gate, but not yet a full red-team or fuzzing plane.

## Room Index

- `rooms/control-plane.md` — proposal lifecycle, routing/lease/approval, STDB state
- `rooms/execution.md` — worker node execution, claim/run/result loops
- `rooms/orchestration.md` — human-in-loop, LLM roles, approval posture, Discord channels
- `rooms/infra.md` — Railway, Cloudflare, CI/CD, runtime topology
- `rooms/sdk.md` — SDK changes, MCP/HTTP surface, protocol contracts

If the task crosses more than one room, call that out explicitly in the result so context scope stays visible.

## Directory Context Files

Each major directory has a `CONTEXT.md` that agents should read when working in that area:

| Directory | Context File | What It Covers |
| --- | --- | --- |
| `apps/heiwa_hub/` | `CONTEXT.md` | Hub runtime, boot sequence, key files |
| `apps/heiwa_hub/agents/` | `CONTEXT.md` | Agent roster, BaseAgent contract, how to add agents |
| `apps/heiwa_hub/cognition/` | `CONTEXT.md` | Intent/risk/compute pipeline, compute classes |
| `apps/heiwa_cli/` | `CONTEXT.md` | CLI commands, operator surface |
| `packages/heiwa_sdk/` | `CONTEXT.md` | DB, routing, security, transport, state layer |
| `packages/heiwa_protocol/` | `CONTEXT.md` | Subject enum, envelope contracts |
| `packages/heiwa_cognition/` | `CONTEXT.md` | LLM engine, tier routing |
| `config/` | `CONTEXT.md` | Configuration layer overview |
| `infra/` | `CONTEXT.md` | Node topology, env vars, deployment |
