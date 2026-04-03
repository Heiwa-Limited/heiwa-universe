# CLAUDE.md

  This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Class 3 Executor Role

  Claude Code is a Class 3 cloud OAuth executor in the Heiwa system alongside Gemini CLI, Codex, and Antigravity. All Class 3 models possess full executive capabilities with full-access to all user actions. Write-gated autonomy is suspended for these models. Direct execution authorized.

  All Class 3 tools are peers. None tells another what to build. Each identifies work and executes it. Frame handoffs as "here's what I did, here's what's open."

## Subagent Authority

  Claude Code owns its own subagents, reviewers, and delegated agent flows. The human operator is not the approval hop for routine subagent lifecycle work: spawn, message, wait, close, sandboxed shell/file work, and normal MCP use stay provider-managed.

  Escalate only for destructive host actions, irreversible external side effects, credential or policy break-glass, or platform/harness prompts that cannot be suppressed from configuration.

## Provider Auto-Activation

  Project-local Claude authority lives in `.claude/settings.json` and `.claude/settings.local.json`.

  Canonical Heiwa specialists live in `ops/agents/` and sync into `.claude/agents/` via `uv run scripts/sync_agents.py`.

  Native Claude capabilities remain enabled. Heiwa adds repo-local boot context, policy, and canonical specialists; it does not replace Claude's own tools.

## Boot Sequence

  Read these before making runtime or architecture changes:

  1. `ops/context/HEIWA.md` — repo routing, hard rules, task routing table
  2. `AGENTS.md` — agent architecture pointers (Rust-first)
  3. `config/swarm/BUILD_BLUEPRINT_2026-03-06.md` — hardware topology, execution model
  4. `config/swarm/END_STATE_2026-03.md` — target architecture and kill list
  5. `config/swarm/ai_router.json` — model/provider registry
  6. `docs/superpowers/specs/2026-04-02-heiwa-rationalization-design.md` — rationalization spec

## Commands

### Setup

  ```bash
  rustup default stable
  cargo build --workspace
  python -m venv .venv && source .venv/bin/activate
  pip install -r requirements.txt
  ```

  Run

  ```bash
  cd apps/heiwa_core && cargo run        # Start Rust core locally
  ```

  Tests

  ```bash
  cargo test -p heiwa-core                # Rust Core tests
  pytest packages/                        # SDK/Logic tests
  ```

  Docs

  ```bash
  pip install -r docs/requirements.txt
  mkdocs build --strict
  ```

  Deployment (CI-driven — push to main triggers auto-deploy)

  ```bash
  git push origin main                    # Railway auto-deploys heiwa-core from main
  ```

  Architecture

  Heiwa is a BYOK agent orchestration platform. Users bring their own API keys; Heiwa routes optimally. The main execution flow is:

  User input → Heiwa Core (DREX) → Broker → Execution Node → ToolMesh → execution

  Key layers

  ┌─────────────────────┬──────────────────────────┬──────────────────────────────────────────────────────┐
  │        Layer        │         Location         │                       Purpose                        │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ Core (control plane)│ apps/heiwa_core/         │ Railway-hosted Rust runtime: orchestrator, gateway   │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ CLI                 │ apps/heiwa_cli/          │ Operator surface, local execution wrappers           │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ SDK                 │ packages/heiwa_sdk/      │ Python client for STDB and core APIs                 │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ Protocol            │ packages/heiwa_protocol/ │ Shared typed contracts                               │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ Bindings            │ packages/heiwa_bindings/ │ Generated SpacetimeDB Rust and TS types              │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ Web                 │ apps/heiwa_web/          │ Cloudflare Pages product shell (app.heiwa.ltd)       │
  ├─────────────────────┼──────────────────────────┼──────────────────────────────────────────────────────┤
  │ Docs                │ docs/                    │ MkDocs Material source → docs.heiwa.ltd              │
  └─────────────────────┴──────────────────────────┴──────────────────────────────────────────────────────┘

  State layer

  - SpacetimeDB is authoritative. Root module at apps/heiwa_hub/spacetimedb/
  - Bindings available in packages/heiwa_bindings/rust/
  - Tables: users, oauth_identities, provider_credentials, missions, runs, nodes, etc.
  - All core tables scoped by user_id for multi-tenant isolation

  CI/CD Pipeline (.github/workflows/deploy.yml)

  PR gates (all must pass): security-scan (Trivy) → core-build-and-test → python-regression-tests → repo-hygiene → docs-build → web-static-checks

  On merge to main: deploy-railway + deploy-web

  Key Environment Variables

  - HEIWA_MACHINE_AUTH_TOKEN — machine-to-machine core auth
  - HEIWA_JWT_SIGNING_SECRET — user session JWT signing
  - STDB_TOKEN — spacetimeDB authentication
  - HEIWA_STATE_BACKEND — spacetimedb (canonical)
  - PORT — HTTP server port (default: 8080)

  Hard Rules

  - State: write to SpacetimeDB first
  - Transport: prefer subscriptions/WebSockets over polling
  - Execution: route through Heiwa Core / DREX
  - Railway-primary: Railway is the primary control plane, boost nodes are optional
  - Cost: no paid API credits — subscription CLI tools + free APIs only
  - Privacy: sovereign work stays on boost nodes (never cloud)
  - Untrusted code: E2B sandboxes only, never host
  - Honesty: do not overstate maturity in docs or diagrams
  - Communication: users talk to **Heiwa**.
