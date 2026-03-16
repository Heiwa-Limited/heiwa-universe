# Heiwa End-State: Objective Conceptualization (March 2026)

## What Heiwa IS

A personal AI operating system that turns ~$83 CAD/month in subscriptions into an always-on, rate-limit-aware execution layer. Railway is the primary plane — self-sufficient with CLI tools, API inference, state, and orchestration. MacBook and WSL are optional boost nodes that add capacity when online.

## Architecture: Railway-Primary

### Railway (Primary Plane — always-on, self-sufficient)

**The Captain** — Event-driven Gemini Flash orchestrator
- Persistent state in SpacetimeDB (focus, decisions, context)
- Proactive communication via Discord
- Delegates to Class 3 CLI tools and free API inference
- Monitors rate limits, deploys, system health
- Coordinates multi-agent workflows via ACP

**Executors** — Class 3 CLI tools installed in Docker
- Claude Code, Gemini CLI, Codex — all installed on Railway
- Spawned as ephemeral subprocesses per task
- Auth via Railway env vars (subscription OAuth)
- Tunable: thinking level, effort, context scope

**API Inference** — Free-tier providers for lightweight tasks
- Google AI Studio (Gemini Flash/Pro) — enrichment, classification, chat
- Cerebras, SiliconFlow, OpenRouter, Groq — overflow inference via direct API
- No paid API credits — subscription-included or free only

**Tool Surface** — MCP servers always available
- Playwright (headless Chromium in container)
- Figma, Notion (cloud APIs)
- Heiwa native tools (status, routing, bench)
- SpacetimeDB state queries

**State Layer**
- SpacetimeDB — authoritative (proposals, nodes, runs, leases, approvals)
- WebSocket subscriptions — no polling
- In-process LocalBusTransport — no NATS, no Redis

### Boost Nodes (Optional — MacBook, WSL)

When online, boost nodes register via `/ws/worker` and add:
- **Ollama** — local GPU inference (M4, RTX 3060)
- **Local filesystem** — access to uncommitted code, local dev environment
- **Docker daemon** — container builds, security scans
- **GPU workloads** — media generation, embeddings
- **Extra execution capacity** — parallel CLI sessions

When offline, nothing breaks. Railway handles everything with cloud tools.

### Identity Plane (config + SpacetimeDB)
- HeiwaCells catalog: typed agent personas with model affinity
- Single SOUL.md identity across all surfaces
- profiles.json materializes cells; ai_router.json routes them

## The Execution Flow

```
Input (CLI / Discord / Webhook / Cron)
  → Captain triages (event-driven Gemini Flash)
  → IntentNormalizer → RiskScorer → ComputeRouter
  → HeiwaClaw (resolve to adapter + provider via direct API or CLI)
  → Rate cascade: pick best available tool with capacity
  → Execute on Railway (CLI subprocess or API inference)
  → If task needs boost (Ollama/filesystem): delegate to boost node
  → Result → SpacetimeDB → Operator surface (CLI / Discord / web)
```

## Protocols

- **MCP** (Model ↔ Tool): Any model calls any tool via MCP bridge
- **ACP** (Agent ↔ Agent): Structured delegation with contracts (task, context, constraints, output format)
- **Skills**: Executable workflow templates that compose MCP tools + ACP delegation

## Rate Cascade (the value engine)

```
1. Gemini CLI     — 50 turns/hr    (free, Google AI Pro)
2. Antigravity    — 35 turns/hr    (free, Google AI Pro)
3. Claude Code    — 40 turns/5hr   ($31/mo subscription)
4. Codex          — 25 turns/hr    ($27/mo subscription)
5. Free APIs      — unlimited-ish  (Cerebras, Google AI Studio, OpenRouter)
6. Ollama         — unlimited      (boost node only)
```

Heiwa spreads work across all providers. Never leaves capacity on the table.

## What Gets Killed

| Kill | Replacement |
| --- | --- |
| NATS | SpacetimeDB subscriptions + in-process bus |
| Polling (tick.py) | STDB subscription callbacks |
| Ad-hoc provider calls | All routing through HeiwaClaw (direct API, no messaging gateway) |
| Local-first execution model | Railway-primary, boost nodes optional |
| Paid API tiers (Claude/OpenAI API) | Subscription CLI tools + free APIs only |
| Individual smoke tests | Unified pytest runner |
| Monolithic Spine | Decomposed agents + Captain orchestrator |

## What Gets Built

| Surface | Purpose |
| --- | --- |
| Captain agent | Always-on Gemini orchestrator on Railway |
| CLI tools in Docker | Railway self-sufficient for Class 3 execution |
| Boost node protocol | MacBook/WSL register capabilities via /ws/worker |
| ACP contracts | Structured agent-to-agent delegation |
| Skill execution engine | YAML workflows Captain can orchestrate |
| STDB subscription layer | Replace all polling with reactive subscriptions |
| MCP tool sharing | MacBook tools accessible from Railway via WebSocket |

## The Four-Class Model

- **Class 1 (CPU):** Shell, git, parse, lint, audit — free, instant
- **Class 2 (GPU):** Local LLM, embeddings, image gen — boost node, free
- **Class 3 (Premium Remote):** Complex reasoning, strategy, code — Railway, subscription-included
- **Class 4 (Cloud Persistence):** Webhooks, schedulers, deploys — Railway, always-on

## Cost Structure

| Resource | Monthly | Role |
|---|---|---|
| Railway Pro | $25 | Primary plane — everything runs here |
| Claude Pro | $31 | Claude Code sessions on Railway |
| ChatGPT Plus | $27 | Codex sessions on Railway |
| Google AI Pro | $0 | Captain + Gemini/Antigravity (free until Dec 2026) |
| Free APIs | $0 | Overflow inference |
| Boost nodes | $0 | Optional capacity (your existing hardware) |

## Success Criteria

- Railway is fully self-sufficient — works with all boost nodes offline
- Captain proactively manages work, communicates via Discord
- Rate cascade spreads work across all 3 CLI tools (Claude Code, Gemini CLI, Codex) + free APIs
- Zero external message brokers
- All state changes flow through SpacetimeDB subscriptions
- Sovereign tasks route to boost nodes only (never cloud)
- Monthly spend stays at ~$83 CAD
- The system is publicly showcasable
