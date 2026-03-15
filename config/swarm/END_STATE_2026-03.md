# Heiwa End-State: Objective Conceptualization (March 2026)

## What Heiwa IS

A personal distributed AI operating system that turns ~$83 CAD/month in OAuth subscriptions into a unified, rate-limit-aware execution layer across four Class 3 cloud models + local inference, with a single identity, a single state layer, and one logical brain regardless of which physical model is executing.

## The Three Planes

### 1. Control Plane (Railway, always-on)
- SpacetimeDB as the sole state layer and message bus (proposals, leases, runs, nodes — all via WebSocket subscriptions, zero polling)
- HTTP/WebSocket ingress for external surfaces (MCP API, status, webhooks, Discord bot)
- In-process local bus for co-located agents — no NATS, no Redis, no external broker
- Crons, schedulers, and uptime-dependent automation
- Cost: stays under $40 CAD/month

### 2. Execution Plane (MacBook + WSL, ephemeral)
- Four Class 3 OAuth CLI executors (Claude, Gemini, Codex, Antigravity) spawned as subprocesses via HeiwaClaw
- Local Ollama inference for sovereign/Class 1-2 work
- E2B sandboxes for untrusted code — never host execution
- Nodes connect, do work, disconnect. No daemons, no mini-servers
- Rate-limit-aware routing: when one group throttles, overflow to another or queue

### 3. Identity Plane (config + SpacetimeDB)
- HeiwaCells catalog: typed agent personas with model affinity, specialization, and risk profiles
- Single SOUL.md identity across all surfaces
- profiles.json materializes cells; ai_router.json routes them
- Every execution is attributable to a cell, a node, and a run

## The Execution Flow (End-State)

```
Human input (CLI / iMessage / MCP)
  → IntentNormalizer (classify intent)
  → RiskScorer (assign risk tier)
  → ComputeRouter (pick class, model, node)
  → HeiwaClaw (resolve to adapter + provider)
  → CLI subprocess execution (claude/gemini/codex/ollama)
  → Result → SpacetimeDB (state written via subscription callback)
  → Operator surface (CLI output / Discord / web)
```

No polling. No NATS. No REST-only multi-turn. WebSocket subscriptions drive all reactive state.

## What Gets Killed

| Kill | Replacement |
| --- | --- |
| NATS | SpacetimeDB WebSocket subscriptions + in-process bus |
| tick.py polling | STDB subscription callbacks |
| Ad-hoc provider calls in agents | All routing through HeiwaClaw/MCP |
| Write-gated autonomy for Class 3 | Full executive capabilities, peer model |
| Individual smoke test scripts | Unified pytest runner |
| Monolithic Spine agent | Decomposed into focused agents (<10 skills each) |
| 3 overlapping routing systems | Single unified compute_router → ai_router.json pipeline |
| Hardcoded /home/devon/heiwa fallback | Env-var driven path resolution |

## What Gets Built (that doesn't exist yet)

| Surface | Purpose |
| --- | --- |
| HeiwaCells marketplace (heiwa install) | Install/registry commands for cell catalog |
| HeiwaBench red-team plane | Adversarial testing beyond release gates |
| SpacetimeDB subscription layer | Replace all polling with reactive subscriptions |
| Unified CLI heiwa probe | Sub-30s health verification across all adapters |
| Cross-platform identity sync | Same SOUL across Mac, WSL, Railway |
| Public ingress pipeline | Single entry point for internet-facing agentic work |

## The Four-Class Model (Unchanged from Blueprint)

- **Class 1 (CPU):** Shell, git, parse, lint, audit — free, instant
- **Class 2 (GPU):** Local LLM, embeddings, image gen — free, seconds
- **Class 3 (Premium Remote):** Complex reasoning, strategy, adversarial review — subscription-bounded
- **Class 4 (Cloud Persistence):** Webhooks, schedulers, deploys, status APIs — Railway

## End-State Success Criteria

- Any Class 3 tool launched from ~ or ~/heiwa instantly knows what it is, what to do, and how to collaborate
- Zero external message brokers running anywhere
- All state changes flow through SpacetimeDB subscriptions
- heiwa bench gates every release automatically
- Sovereign tasks never leave the local network
- Monthly cloud spend stays under $40 CAD
- The system is publicly showcasable and can accept internet-facing work

## Transition Note

NATS is debt to retire, not infrastructure to harden.
