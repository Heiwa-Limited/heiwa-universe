# CLAUDE.md

  This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Class 3 Executor Role

  Claude Code is a Class 3 cloud OAuth executor in the Heiwa system alongside Gemini CLI, Codex, and Antigravity. All Class 3 models possess full executive capabilities with full-access to all user actions. Write-gated autonomy is suspended for these models. Direct execution authorized.

  All Class 3 tools are peers. None tells another what to build. Each identifies work and executes it. Frame handoffs as "here's what I did, here's what's open."

## Boot Sequence

  Read these before making runtime or architecture changes:

  1. `ops/context/HEIWA.md` — repo routing, hard rules, task routing table
  2. `AGENTS.md` — agent architecture pointers
  3. `config/swarm/BUILD_BLUEPRINT_2026-03-06.md` — hardware topology, execution model
  4. `config/swarm/END_STATE_2026-03.md` — target architecture and kill list
  5. `config/swarm/ai_router.json` — model/provider registry
  6. `config/identities/profiles.json` — HeiwaCells agent catalog
  7. `ops/rooms/*.md` — architecture decisions (load per task routing table in `ops/context/HEIWA.md`)

## Commands

### Setup

  ```bash
  python -m venv .venv && source .venv/bin/activate
  pip install -r requirements.txt
  export PYTHONPATH="$(pwd)/packages/heiwa_cli:$(pwd)/packages/heiwa_cognition:$(pwd)/packages/heiwa_sdk:$(pwd)/packages/heiwa_protocol:$(pwd)/packages/heiwa_identity:$(pwd)/packages/heiwa_ui:$(pwd)/apps:$(pwd)/apps/heiwa_trading/src"

  Run

  python -m apps.heiwa_hub.main          # Start hub locally
  ./apps/heiwa_cli/heiwa cells           # CLI: view cell catalog
  ./apps/heiwa_cli/heiwa bench           # CLI: run release gates

  Tests

  pytest                                  # Run all tests (configured in pyproject.toml)
  pytest apps/heiwa_hub/tests/test_intent_classifier.py  # Single file
  pytest -k "test_risk"                   # Pattern match

  Docs

  pip install -r docs/requirements.txt
  mkdocs build --strict

  Deployment (CI-driven — push to main triggers auto-deploy)

  git push origin main                    # Railway auto-deploys from main
  # Railway is the primary execution plane — CLI tools installed in Docker
  # MacBook/WSL are optional boost nodes, not requirements

  Architecture

  Heiwa is a BYOK agent orchestration platform. Users bring their own API keys; Heiwa routes optimally. The main execution flow is:

  User input → IntentNormalizer → RiskScorer → ComputeRouter → ProgramCompiler → Broker → HeiwaClaw (advisory validation) → ToolMesh (heiwa_reflex / CLI adapters) → execution

  Key layers

  ┌─────────────────────┬──────────────────────────┬──────────────────────────────────────────────────────┐
  │        Layer        │         Location         │                       Purpose                        │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ Hub (control plane) │ apps/heiwa_hub/          │ Railway-hosted runtime: agents, MCP/HTTP API, health │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ CLI                 │ apps/heiwa_cli/heiwa     │ Operator surface, local execution wrappers           │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ SDK                 │ packages/heiwa_sdk/      │ State, security, gateway, routing, scheduler         │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ Protocol            │ packages/heiwa_protocol/ │ Shared typed contracts (BrokerRouteRequest/Result)   │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ Bindings            │ packages/heiwa_bindings/ │ Generated SpacetimeDB types                          │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ Web                 │ apps/heiwa_web/          │ app.heiwa.ltd — dashboard, key vault, mission history │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ Docs                │ docs/                    │ MkDocs Material source → docs.heiwa.ltd              │
  └─────────────────────┴──────────────────────────┴──────────────────────────────────────────────────────┘

  Agents (apps/heiwa_hub/agents/)

  All extend BaseAgent from base.py:
  - Spine — fleet orchestration, node registry, heartbeats, request routing
  - HeiwaClaw — unified living agent: observes system, DMs operator, executes tasks via OpenClaw (spawning mechanism)
  - Telemetry — system metrics collection and reporting
  - Messenger — Discord integration (optional, auto-detected)

  Cognition pipeline (packages/heiwa_cognition/)

  - intent_normalizer.py — classifies user input into intent enums (build, deploy, research, audit, etc.)
  - risk_scorer.py — assigns risk level
  - compute_router.py — routes to compute class/worker/model/tier; enforces privacy-first and cost gates
  - program_compiler.py — compiles IntentProfile + ComputeRoute + raw text into typed ExecutionProgram (deterministic, no LLM)

  Protocol contracts (packages/heiwa_protocol/)

  - routing.py — BrokerRouteRequest/Result typed contracts, carries optional ExecutionProgram
  - program.py — ExecutionProgram typed contract: objective, steps, constraints, acceptance, budget, rollback, scope, tools_allowed

  Execution gateway (packages/heiwa_sdk/)

  - heiwaclaw/ — OpenClaw spawning mechanism: resolves BrokerRouteResult → OpenClawDispatch (tool, adapter, provider, transport)
  - tool_mesh.py — executes selected adapter with environment (heiwa_ops, heiwa_reflex)
  - routing.py — compute routing logic
  - db.py — multi-backend DB abstraction (SpacetimeDB, Postgres, SQLite via HEIWA_STATE_BACKEND)
  - spacetimedb.py — native SpacetimeDB client bridge
  - tick.py — proposal scheduler/lifecycle

  State layer

  - SpacetimeDB is authoritative. Rust module at apps/heiwa_hub/heiwaproductiondb/spacetimedb/
  - Backend selected via HEIWA_STATE_BACKEND env var
  - Tables: users, oauth_identities, provider_credentials, billing_events, proposals, missions, runs, nodes, capability_leases, approval_requests, approval_decisions, pods
  - All core tables scoped by user_id for multi-tenant isolation

  CI/CD Pipeline (.github/workflows/deploy.yml)

  PR gates (all must pass): security-scan (Trivy) → hub-smoke-tests → repo-hygiene → docs-build → web-static-checks

  On merge to main: deploy-railway + deploy-web

  Key Environment Variables

  - HEIWA_AUTH_TOKEN — digital barrier auth validation
  - HEIWA_STATE_BACKEND — spacetimedb | postgres | sqlite
  - HEIWA_ENABLE_BROKER — enable enrichment agent (default: true)
  - OLLAMA_BASE_URL — local LLM endpoint
  - PORT — HTTP server port (default: 8080)

  Hard Rules

  - State: write to SpacetimeDB first
  - Transport: prefer subscriptions/WebSockets over polling
  - Execution: route through HeiwaClaw/MCP, not ad-hoc provider calls
  - Railway-primary: Railway is the primary execution plane, boost nodes are optional
  - Cost: no paid API credits — subscription CLI tools + free APIs only
  - Privacy: sovereign work stays on boost nodes (never cloud)
  - Untrusted code: E2B sandboxes only, never host
  - Honesty: do not overstate maturity in docs or diagrams
