# GEMINI.md — heiwa-universe

This repository builds Heiwa, a local-first AI runtime and enterprise platform. Gemini CLI is one wrapped provider surface inside Heiwa, not the product itself.

Naming:

- **Heiwa** = product/app/runtime/CLI/packages/docs.
- **Heiwa Limited** = company/publisher/legal identity.
- **Heiwa Universe** = this open-source repo, `Strategizing/heiwa-universe`.

## Gemini's Role Here

- Gemini CLI is a peer executor alongside Claude Code, Codex, Antigravity, and local model runtimes.
- Gemini owns its own native auth flows, system prompts, cloud model inventory, quota behavior, and tool semantics.
- Heiwa adds repo-local context, routing, evidence, shell ergonomics, and cross-provider normalization.
- Do not imply that Heiwa owns Gemini's inference internals.

## Required Reading

Before touching runtime or architecture work, read in this order:

1. `HEIWA.md`
2. `AGENTS.md`
3. `.gemini/settings.json`
4. `.gemini/policies/heiwa-executive.toml`

## Current Product Truth

- The installed `heiwa` runtime is the current product center.
- `apps/heiwa_shell/` is the main operator surface in this repo.
- `apps/heiwa_core/` contains the Rust execution kernel and hosted runtime path.
- STDB-facing active work lives in `apps/heiwa_core/src/stdb/`, `apps/heiwa_orchestrator/src/stdb/`, and `crates/heiwa_stdb/`.
- `legacy/apps/heiwa_hub/` is quarantined migration/reference material, not a current mutation target.
- Web and `/code` surfaces are later work. Do not overstate them.

## Provider Truth

Heiwa wraps provider-owned runtimes:

- Claude Code
- Codex
- Gemini CLI
- Antigravity
- Ollama and later local runtimes

Integration maturity differs across those providers. Discovery is not the same as full execution parity.

## Shared Peer Truth

Use corrected peer framing before architecture or parity work:

- Hermes is Python, server/VPS-friendly, terminal-first. It proves learning loop,
  skills, FTS5 recall, Honcho user modeling, messaging gateway, cron delivery,
  MCP, provider switching, and terminal backends. Do not call it a worker mesh.
- OpenHuman is Rust + Tauri/CEF with local memory plus managed default services.
  It proves consumer desktop onboarding, Memory Tree, Obsidian vault,
  Composio/OAuth integrations, TokenJuice, and voice/meeting surface. Do not
  call it pure local-first.
- Heiwa's defensible difference: provider-peer MacBook owner seat, local runtime
  authority, approvals, receipts, STDB evidence sync, and provider-owned runtime
  truth.
- Biggest current gap: connector/tool breadth and compression/learning loop.
  Do not imply parity until code proves it.

## Engineering Standards (2026-03 BYOK Update)

### 1. Identity & Multi-tenancy

- **Primary human operator (Devon)**: `owner_id="0"`.
- **System identities**: `operator` and `local-operator` are equivalent to `0` for system-wide key access.
- **Helper**: Always use `is_system_operator(owner_id)` from `heiwa_protocol.routing` to check privileges.
- **Auth**: `HEIWA_ADMIN_ID_MAPPINGS` (e.g., `discord:123456789=0`) handles admin promotion and relinking.

### 2. Security & Credentials

- **Vault First**: All provider keys MUST be resolved via `UserVault` in SpacetimeDB for `owner_id != system`.
- **Scrubbing**: `ToolMesh` uses `SAFE_ENV_ALLOWLIST`. Never expose `HEIWA_MASTER_KEY` or `RAILWAY_AUTH_TOKEN` to child processes.
- **BYOK**: Strict enforcement. If a user key is missing, return `BLOCKED_AUTH`.

### 3. Execution Patterns

- **Propagation**: `owner_id` must be carried in `BrokerRouteRequest` and `BrokerRouteResult`.
- **Status Mapping**: Map authentication failures to `BLOCKED_AUTH` in `OpenClaw` and narrate specifically in `HeiwaClawAgent`.

## Commands

```bash
cargo build --workspace
cargo test -p heiwa-shell --test smoke -- --nocapture
cargo test -p heiwa-loop -- --nocapture
```

## Hard Rules

- local-first truth over hosted-first framing
- provider-owned semantics stay provider-owned
- SpacetimeDB is backend authority, not a normal operator surface
- Railway is support infra, not the product center
- honesty over maturity theater
- State: write to SpacetimeDB first
- Transport: prefer subscriptions/WebSockets over polling
- Cost: cheapest acceptable route first
- Privacy: sovereign work stays local-first
- Untrusted code: E2B sandboxes only, never host
