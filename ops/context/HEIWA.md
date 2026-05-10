# HEIWA.md — Agent Routing Map

Companion to repo-root [`HEIWA.md`](../../HEIWA.md). That document is the canonical architecture truth. This file is the short agent-facing routing map.

## Read Order

Read these in order before making runtime or architecture changes:

1. Repo-root [`HEIWA.md`](../../HEIWA.md) — canonical architecture truth
2. [`AGENTS.md`](../../AGENTS.md) — agent contract
3. [`ops/context/SOUL.md`](SOUL.md) — continuity layer
4. [`config/swarm/BUILD_BLUEPRINT_2026-03-06.md`](../../config/swarm/BUILD_BLUEPRINT_2026-03-06.md)
5. [`config/swarm/END_STATE_2026-03.md`](../../config/swarm/END_STATE_2026-03.md) — target architecture and kill list
6. [`config/swarm/ai_router.json`](../../config/swarm/ai_router.json)
7. [`config/identities/profiles.json`](../../config/identities/profiles.json)
8. The room files relevant to the task under [`ops/rooms/`](../rooms/)

## Architecture Of Record

- **MacBook (macOS)**: Owner/operator seat. Primary local runtime host.
- **Local `heiwa` runtime**: Installed on each operator machine; owns routing and execution locally.
- **SpacetimeDB**: Backend authority plane for cross-device adjudication and evidence. Not a normal operator surface.
- **GitHub**: Distribution surface — Releases, Pages, Actions, homebrew tap.
- **Consolidated Monorepo**: `~/heiwa-universe` is the canonical active workspace. Older `~/heiwa` references are compatibility debt to retire, not the source of truth.

A cloud/VPS plane is deferred until traction warrants it; the client-only architecture is the current truth.

## Harness Scaffolding (Ground Truth)

The system is anchored by machine-readable files that persist across agent sessions:

- `docs/superpowers/status/feature_list.json`: The inviolable truth of system capabilities.
- `docs/superpowers/status/progress.md`: The multi-session handoff log.
- `scripts/init_env.sh`: Standardized environment initialization for all nodes.

## Provider Control Surfaces

- Repo-local provider posture lives in `.codex/`, `.claude/`, and `.gemini/`.
- Canonical cross-runtime specialists live in `ops/agents/` and sync into provider-native discovery surfaces with `uv run scripts/sync_agents.py`.
- Native provider tools stay enabled. Heiwa only adds boot order, canonical context, policies, and specialist wrappers.

## Hard Rules

- **State Authority**: SpacetimeDB is the authority plane for cross-device state. Local SQLite (e.g. `~/.heiwa/state.db`) is the legitimate per-machine ledger for quota, history, and run traces; it is not retired.
- **Target Fidelity**: Develop for the operator's local machine first. Cross-device sync flows through STDB when it matters.
- **Transport**: Prefer subscriptions and WebSockets over polling.
- **Execution**: Route model and tool execution through `HeiwaClaw` / MCP.
- **Economy**: Cheapest acceptable route first.
- **Privacy**: Sovereign work stays on local boost nodes.

## Task Routing Table

| Intent Class | Default Runtime | Primary Tool Surface | Primary Room |
| --- | --- | --- | --- |
| `chat` / `general` | local `heiwa` runtime | `heiwa_claw` | `ops/rooms/orchestration.md` |
| `build` / `fix` / `review` | local operator seat, escalate to provider CLIs as needed | native Class 3 agent lanes | `ops/rooms/execution.md` |
| `research` / `strategy` | local `heiwa` runtime unless sovereign | `heiwa_claw` / broker enrichment | `ops/rooms/orchestration.md` |
| `deploy` / `operate` / `automate` | local `heiwa` runtime + GitHub Actions | control-plane services | `ops/rooms/infra.md` |
| `audit` / `files` | local-first | deterministic ops / local execution | `ops/rooms/sdk.md` |

Routing details live in `config/swarm/ai_router.json`, current Rust runtime crates, and legacy references under `legacy/packages/heiwa_cognition/` when migrating older Python routing behavior. Use the room files as the human-readable map before changing runtime behavior.

## Transitional Boundaries

- Proposal / lease / RFC flow is still partly compatibility-SQL and polling-shaped.
- `HeiwaCells` is a real catalog, but not yet a full installer/marketplace surface.
- `HeiwaBench` is a real release gate, but not yet a full red-team or fuzzing plane.

## Room Index

- `ops/rooms/control-plane.md` — proposal lifecycle, routing/lease/approval, STDB state
- `ops/rooms/execution.md` — worker node execution, claim/run/result loops
- `ops/rooms/orchestration.md` — human-in-loop, LLM roles, approval posture, Discord channels
- `ops/rooms/infra.md` — GitHub distribution, Cloudflare edge, CI/CD, runtime topology
- `ops/rooms/sdk.md` — SDK changes, MCP/HTTP surface, protocol contracts

If the task crosses more than one room, call that out explicitly in the result so context scope stays visible.

## Directory Context Files

Each major directory has a `CONTEXT.md` that agents should read when working in that area:

| Directory | Context File | What It Covers |
| --- | --- | --- |
| `apps/heiwa_core/` | `CONTEXT.md` | Rust execution kernel, routing, receipts, hosted runtime path |
| `apps/heiwa_orchestrator/` | `CONTEXT.md` | DREX orchestration, scoring, persistence, STDB-facing runtime work |
| `apps/heiwa_shell/` | `CONTEXT.md` | Installed `heiwa` runtime and shell surface |
| `legacy/apps/heiwa_hub/` | `CONTEXT.md` | Quarantined legacy Hub reference; repair only when promoting or migrating |
| `legacy/apps/heiwa_cli/` | `CONTEXT.md` | Legacy CLI reference |
| `apps/heiwa_trading/` | `CONTEXT.md` | Trading cockpit, supervisor, strategy engine |
| `apps/heiwa_dj/` | `CONTEXT.md` | Archived — shipped v1.7.0 standalone app |
| `packages/heiwa_sdk/` | `CONTEXT.md` | DB, routing, security, transport, state layer |
| `packages/heiwa_protocol/` | `CONTEXT.md` | Subject enum, envelope contracts |
| `legacy/packages/heiwa_cognition/` | `CONTEXT.md` | Legacy LLM engine, tier routing reference |
| `config/` | `CONTEXT.md` | Configuration layer overview |
| `infra/` | `CONTEXT.md` | Local vs platform ops split, per-machine bootstrap |
