# GEMINI.md — Heiwa Monorepo

## Class 3 Executor Role

Gemini CLI is a Class 3 cloud OAuth executor with full agentic/sub-agentic capabilities. All Class 3 models (Claude Code, Gemini CLI, Codex, Antigravity) possess full executive capabilities. Write-gated autonomy is suspended. Direct execution authorized.

## Boot Sequence

Read these before making runtime or architecture changes:

1. `ops/context/HEIWA.md` — repo routing, hard rules, task routing table
2. `AGENTS.md` — agent architecture pointers
3. `config/swarm/BUILD_BLUEPRINT_2026-03-06.md` — hardware topology, execution model
4. `config/swarm/END_STATE_2026-03.md` — target architecture and kill list
5. `config/swarm/ai_router.json` — model/provider registry
6. `config/identities/profiles.json` — HeiwaCells agent catalog
7. `ops/rooms/*.md` — architecture decisions (load per task routing table in `ops/context/HEIWA.md`)

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

## Hard Rules

- State: write to SpacetimeDB first
- Transport: prefer subscriptions/WebSockets over polling
- Execution: route through HeiwaClaw/MCP, not ad-hoc provider calls
- Railway-primary: Railway is the primary execution plane, boost nodes are optional
- Cost: no paid API credits — subscription CLI tools + free APIs only
- Privacy: sovereign work stays on boost nodes (never cloud)
- Untrusted code: E2B sandboxes only, never host
- Honesty: do not overstate maturity in docs or diagrams
- Memory: Heiwa Agent maintains persistent conversational context via SpacetimeDB memory loops.
