# GEMINI.md — Heiwa Monorepo

## Class 3 Executor Role

Gemini CLI is a Class 3 cloud OAuth executor with full agentic/sub-agentic capabilities. All Class 3 models (Claude Code, Gemini CLI, Codex, Antigravity) possess full executive capabilities. Write-gated autonomy is suspended. Direct execution authorized.

## Boot Sequence

Read these before making runtime or architecture changes:

1. `ops/context/HEIWA.md` — repo routing, hard rules, task routing table
2. `AGENTS.md` — agent architecture pointers (Rust-first)
3. `config/swarm/BUILD_BLUEPRINT_2026-03-06.md` — hardware topology, execution model
4. `config/swarm/END_STATE_2026-03.md` — target architecture and kill list
5. `config/swarm/ai_router.json` — model/provider registry
6. `docs/superpowers/specs/2026-04-02-heiwa-rationalization-design.md` — rationalization spec

## Peer Collaboration

All Class 3 tools are peers. None tells another what to build. Each identifies work and executes it. Frame handoffs as "here's what I did, here's what's open."

## Architecture

Heiwa is a distributed AI operating system. The main execution flow is:

```
User input → Heiwa Core (DREX) → Broker → Execution Node → ToolMesh → execution
```

| Layer | Location | Purpose |
| --- | --- | --- |
| Core (control plane) | `apps/heiwa_core/` | Railway-hosted Rust runtime: orchestrator, WS gateway, auth |
| CLI | `apps/heiwa_cli/` | Operator surface, local execution wrappers |
| SDK | `packages/heiwa_sdk/` | Python client for STDB and core APIs |
| Protocol | `packages/heiwa_protocol/` | Shared typed contracts |
| Bindings | `packages/heiwa_bindings/rust/` | Generated SpacetimeDB Rust types |
| Web | `apps/heiwa_web/` | Cloudflare Pages product shell (app.heiwa.ltd) |
| Docs | `docs/` | MkDocs Material source → docs.heiwa.ltd |

## Commands

```bash
# Setup
rustup default stable
cargo build --workspace

# Run Core locally
cd apps/heiwa_core && cargo run

# Tests
cargo test -p heiwa-core                   # Rust Core tests
pytest packages/                           # SDK/Logic tests

# Deploy (CI-driven — push to main triggers Railway auto-deploy)
git push origin main
```

## Hard Rules

- State: write to SpacetimeDB first
- Transport: prefer subscriptions/WebSockets over polling
- Execution: route through Heiwa Core / DREX
- Railway-primary: Railway is the primary control plane, boost nodes are optional
- Cost: no paid API credits — subscription CLI tools + free APIs only
- Privacy: sovereign work stays on boost nodes (never cloud)
- Untrusted code: E2B sandboxes only, never host
- Honesty: do not overstate maturity in docs or diagrams
- Communication: users talk to **Heiwa**. "Captain" is an internal system identity.
- Memory: Heiwa maintains persistent conversational context via SpacetimeDB.
