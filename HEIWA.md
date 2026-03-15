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

- Railway is the always-on control plane and cloud host.
- SpacetimeDB is the authoritative state layer.
- The MacBook is the high-trust orchestrator and human-in-the-loop surface.
- WSL/Ubuntu is the worker node and development/runtime execution surface.
- Cloudflare is the public docs/marketing shell, not the trust boundary.
- Discord is a notification and human-facing query surface, not a trust boundary.

## Live Seed Surfaces

- `HeiwaCells`: materialized from `config/identities/profiles.json`
- `HeiwaBench`: checked-in release-gate suites for routes and cell selection

These are live through:

- CLI: `apps/heiwa_cli/heiwa`
- MCP/HTTP: `apps/heiwa_hub/mcp_server.py`
- CI: `.github/workflows/deploy.yml`

## Task Routing Table

Load only the rooms needed for the task unless the work is architectural.

| Task Class | Load Rooms | Skip |
| --- | --- | --- |
| Proposal lifecycle | `rooms/control-plane.md`, `rooms/sdk.md` | `rooms/infra.md`, `rooms/execution.md` |
| Worker node execution | `rooms/execution.md`, `rooms/infra.md` | `rooms/orchestration.md` |
| SDK surface changes | `rooms/sdk.md`, `rooms/control-plane.md` | `rooms/infra.md` |
| CI / deploy changes | `rooms/infra.md`, `rooms/execution.md` | `rooms/orchestration.md` |
| Orchestration / human-in-loop | `rooms/orchestration.md`, `rooms/control-plane.md` | `rooms/infra.md` |
| Architecture / design | all rooms | none |

## Hard Rules

- Write important state to SpacetimeDB first.
- Prefer subscriptions and WebSockets over polling.
- Route model and tool execution through `HeiwaClaw` / MCP, not ad hoc provider calls in agent logic.
- Cheapest acceptable route first.
- Sovereign work must stay local-first.
- Do not overstate maturity in docs or diagrams.

## Transitional Boundaries

- Proposal / lease / RFC flow is still partly compatibility-SQL and polling-shaped.
- `HeiwaCells` is a real catalog, but not yet a full installer/marketplace surface.
- `HeiwaBench` is a real release gate, but not yet a full red-team or fuzzing plane.

## Room Index

- `rooms/control-plane.md`
- `rooms/execution.md`
- `rooms/orchestration.md`
- `rooms/infra.md`
- `rooms/sdk.md`

If the task crosses more than one room, call that out explicitly in the result so context scope stays visible.
