# GEMINI.md — Heiwa Monorepo

## Class 3 Executor Role

Gemini CLI is a Class 3 cloud OAuth executor with full agentic/sub-agentic capabilities. All Class 3 models (Claude Code, Gemini CLI, Codex, Antigravity) possess full executive capabilities. Write-gated autonomy is suspended. Direct execution authorized.

## Boot Sequence

Read these before making runtime or architecture changes:

1. `HEIWA.md` — repo routing, hard rules, task routing table
2. `AGENTS.md` — agent architecture pointers
3. `config/swarm/BUILD_BLUEPRINT_2026-03-06.md` — hardware topology, execution model
4. `config/swarm/END_STATE_2026-03.md` — target architecture and kill list
5. `config/swarm/ai_router.json` — model/provider registry
6. `config/identities/profiles.json` — HeiwaCells agent catalog
7. `rooms/*.md` — architecture decisions (load per task routing table in HEIWA.md)

## Peer Collaboration

All Class 3 tools are peers. None tells another what to build. Each identifies work and executes it. Frame handoffs as "here's what I did, here's what's open."

## Architecture

Heiwa is a distributed AI operating system. The main execution flow is:

```
User input → IntentNormalizer → RiskScorer → ComputeRouter → Broker → HeiwaClaw → ToolMesh → execution
```

| Layer | Location | Purpose |
| --- | --- | --- |
| Hub (control plane) | `apps/heiwa_hub/` | Railway-hosted runtime: agents, MCP/HTTP API, health |
| CLI | `apps/heiwa_cli/heiwa` | Operator surface, local execution wrappers |
| SDK | `packages/heiwa_sdk/` | State, security, gateway, routing, scheduler |
| Protocol | `packages/heiwa_protocol/` | Shared typed contracts (BrokerRouteRequest/Result) |
| Bindings | `packages/heiwa_bindings/` | Generated SpacetimeDB types |
| Web | `apps/heiwa_web/` | Cloudflare Pages marketing/status shell |
| Docs | `docs/` | MkDocs Material source → docs.heiwa.ltd |

## Agents (`apps/heiwa_hub/agents/`)

All extend BaseAgent from base.py:
- Spine — fleet orchestration, node registry, heartbeats, request routing
- Executor — claims and executes tasks via HeiwaClaw + ToolMesh
- Captain — always-on event-driven orchestrator (Gemini Flash). Monitors health, delegates, communicates
- Telemetry — system metrics collection and reporting
- Messenger — Discord integration (optional, auto-detected)

## Commands

```bash
# Setup
python -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
export PYTHONPATH="$(pwd)/packages/heiwa_cli:$(pwd)/packages/heiwa_cognition:$(pwd)/packages/heiwa_sdk:$(pwd)/packages/heiwa_protocol:$(pwd)/packages/heiwa_identity:$(pwd)/packages/heiwa_ui:$(pwd)/apps"

# Run
python -m apps.heiwa_hub.main          # Start hub locally
./apps/heiwa_cli/heiwa cells           # CLI: view cell catalog
./apps/heiwa_cli/heiwa bench           # CLI: run release gates

# Tests
pytest                                    # Run all tests (configured in pyproject.toml)
pytest apps/heiwa_hub/tests/test_intent_classifier.py  # Single file
pytest -k "test_risk"                     # Pattern match

# Deploy (CI-driven — push to main triggers Railway auto-deploy)
git push origin main
```

## Task Routing Table

Load only the rooms needed for the task:

| Task Class | Load Rooms | Skip |
| --- | --- | --- |
| Proposal lifecycle | `rooms/control-plane.md`, `rooms/sdk.md` | `rooms/infra.md`, `rooms/execution.md` |
| Worker node execution | `rooms/execution.md`, `rooms/infra.md` | `rooms/orchestration.md` |
| SDK surface changes | `rooms/sdk.md`, `rooms/control-plane.md` | `rooms/infra.md` |
| CI / deploy changes | `rooms/infra.md`, `rooms/execution.md` | `rooms/orchestration.md` |
| Orchestration / human-in-loop | `rooms/orchestration.md`, `rooms/control-plane.md` | `rooms/infra.md` |
| Architecture / design | all rooms | none |

## Hard Rules

- State: write to SpacetimeDB first
- Transport: prefer subscriptions/WebSockets over polling
- Execution: route through HeiwaClaw/MCP, not ad-hoc provider calls
- Railway-primary: Railway is the primary execution plane, boost nodes are optional
- Cost: no paid API credits — subscription CLI tools + free APIs only
- Privacy: sovereign work stays on boost nodes (never cloud)
- Untrusted code: E2B sandboxes only, never host
- Honesty: do not overstate maturity in docs or diagrams
