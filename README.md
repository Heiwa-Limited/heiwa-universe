# Heiwa

[![Railway](https://img.shields.io/badge/runtime-Railway-5a3cc8?style=flat-square&logo=railway)](https://railway.app)
[![SpacetimeDB](https://img.shields.io/badge/state-SpacetimeDB-0c73d8?style=flat-square)](https://spacetimedb.com)
[![Web](https://img.shields.io/badge/dashboard-app.heiwa.ltd-000000?style=flat-square)](https://app.heiwa.ltd)
[![Discord](https://img.shields.io/badge/interface-Discord-5865F2?style=flat-square&logo=discord)](https://discord.com)

Heiwa is a BYOK (Bring Your Own Keys) agent orchestration platform. Connect your own AI provider API keys and OAuth credentials. Heiwa wraps every inference provider, scores intent and risk, and routes each task to the optimal model and provider -- across multi-step agent workflows.

Access via [app.heiwa.ltd](https://app.heiwa.ltd) (web dashboard) and Discord DMs.

## What Heiwa Does

Users bring their own API keys (OpenAI, Anthropic, Google, etc.). Heiwa handles the rest:

- **Intent classification** -- understands what you want to do
- **Risk scoring** -- evaluates safety and cost implications
- **Optimal routing** -- picks the best model/provider for each step, using your keys and rate limits
- **Multi-step orchestration** -- chains tasks into agent workflows with structured execution programs
- **Multi-tenant state** -- every user's credentials, runs, and results are scoped and isolated

No vendor lock-in. No platform inference costs passed to you. Your keys, your budget, Heiwa's routing intelligence.

## Architecture

| Component | Location | Role |
|-----------|----------|------|
| **Core** | `apps/heiwa_core/` | Railway-hosted Rust runtime authority -- auth, worker ingress, DREX routing, API/WS surface |
| **SpacetimeDB** | `apps/heiwa_hub/spacetimedb/` | Multi-tenant state engine -- users, credentials, proposals, runs, billing |
| **Web** | `apps/heiwa_web/` | Cloudflare-delivered public/product shell surface -- static today, TypeScript baseline added |
| **Discord** | `apps/heiwa_hub/agents/` | Legacy/porting surface -- high-value patterns still being migrated |
| **SDK** | `packages/heiwa_sdk/` | State, security, gateway, routing, scheduler |
| **Cognition** | `packages/heiwa_cognition/` | Intent normalizer, risk scorer, compute router, program compiler |
| **Protocol** | `packages/heiwa_protocol/` | Shared typed contracts and schemas |
| **Bindings** | `packages/heiwa_bindings/` | Generated SpacetimeDB clients (Rust/TypeScript/Python) |

## Runtime Topology

```mermaid
graph TD
    A["app.heiwa.ltd"] --> B["Heiwa Core (Railway)"]
    C["Discord"] --> B
    B --> D["SpacetimeDB"]
    B --> E["Provider APIs (user keys)"]
    B --> F["CLI Tools (platform keys)"]
    D --> G["User State / Credentials / Billing"]
```

The active runtime authority is **Heiwa Core** on Railway. Python hub code remains as legacy/reference and for transitional surfaces while the mesh and product shell continue moving onto the Rust + TypeScript spine.

## Verticals

**Trading** -- Polymarket analysis and paper-trading. Heiwa runs market scans using your inference budget, scores opportunities, and surfaces results via Discord DM and web dashboard. Source: `apps/heiwa_trading/`.

**Autoresearch** -- Karpathy-style autonomous research loops. Define a research question, Heiwa orchestrates multi-model deep dives and delivers structured findings.

## Quick Start

```bash
cd ~/heiwa-universe
bash scripts/check_runtime_baseline.sh
npm install
npm run typecheck
cargo run -p heiwa-core
```

## Product Graph

The root [`justfile`](justfile) is the product build contract. If a task is not represented there, it is not part of the hosted product graph.

- Product surfaces: `apps/`, `packages/`, `config/`, `infra/`, `docs/`, `scripts/`
- Operator-only context/tooling: `ops/`

## Key Manifests

- [`config/swarm/BUILD_BLUEPRINT_2026-03-06.md`](config/swarm/BUILD_BLUEPRINT_2026-03-06.md)
- [`config/swarm/ai_router.json`](config/swarm/ai_router.json)
- [`config/identities/profiles.json`](config/identities/profiles.json)
- [`config/swarm/domain_plan.md`](config/swarm/domain_plan.md)
- [`docs/standards/runtime-baseline.md`](docs/standards/runtime-baseline.md)
- [`HEIWA.md`](HEIWA.md)
