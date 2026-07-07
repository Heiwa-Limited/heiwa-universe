# Heiwa Agent Persistent Memory & Railway Cloud-First Deployment

> **Status:** Approved design — ready for implementation planning
> **Author:** Claude Code (Class 3 Executor) with input from Gemini CLI
> **Date:** 2026-03-17

## Goal

Give Heiwa Agent (the always-on orchestrator, formerly "Captain") persistent cross-session memory via SpacetimeDB, deploy the full stack cloud-first on Railway with persistent volumes, and establish the agent identity/naming conventions that make Heiwa a proper distributed AI operating system connectable from any device.

## Architecture Overview

**Tri-Layer Hybrid Memory** (originated by Gemini CLI, refined collaboratively):

1. **Structured Ledger (SpacetimeDB)** — Every interaction stored as raw messages with metadata. Source of truth for exact history.
2. **Semantic Index (Knowledge Embeddings)** — Compressed summaries embedded into the existing `knowledge_embeddings` table during compression. Enables "remember when we discussed X last week" via semantic search.
3. **Active Context (In-Memory Window)** — Sliding window of recent messages + retrieved summaries + system state, assembled per-interaction and passed to the LLM.

**Compression Strategy:** Structured Storage + LLM-on-Compress (Approach B). Raw messages stored instantly with zero LLM cost. LLM invoked only at two triggers: rolling window overflow and daily digest generation.

---

## 1. STDB Schema — New Tables

**Target file:** `apps/heiwa_hub/spacetimedb/src/lib.rs` (the production Rust module, currently 1,987 lines with 27 tables). NOT `heiwaproductiondb/spacetimedb/src/lib.rs` which is a stub.

### `captain_messages` — Raw Conversation Ledger

| Column       | Type          | Purpose                                                     |
| ------------ | ------------- | ----------------------------------------------------------- |
| `message_id` | `String` (PK) | UUID per message                                            |
| `session_id` | `String`      | Groups messages within a boot-to-shutdown or daily boundary |
| `role`       | `String`      | `operator` or `agent`                                       |
| `content`    | `String`      | Raw message text                                            |
| `timestamp`  | `u64`         | Unix ms                                                     |
| `source`     | `String`      | `discord_dm`, `cli`, `api` — supports any device            |
| `compressed` | `bool`        | Marks messages that have been rolled into a summary         |

Per-message granularity (not per-turn) because Heiwa Agent sends proactive messages (status updates, alerts, task observations) that don't pair with an operator message. Matches Discord's data model with no impedance mismatch.

### `captain_summaries` — Compressed Memory

| Column                | Type          | Purpose                             |
| --------------------- | ------------- | ----------------------------------- |
| `summary_id`          | `String` (PK) | UUID                                |
| `summary_type`        | `String`      | `rolling` or `daily_digest`         |
| `content`             | `String`      | LLM-generated summary text          |
| `message_range_start` | `u64`         | Earliest message timestamp covered  |
| `message_range_end`   | `u64`         | Latest message timestamp covered    |
| `messages_compressed` | `u32`         | Count of raw messages this replaced |
| `created_at`          | `u64`         | When the summary was generated      |

Daily digests are also dumped to maincloud for long-term archive and boost node hydration.

### `captain_focus` — Active Tracking

| Column         | Type          | Purpose                                                   |
| -------------- | ------------- | --------------------------------------------------------- |
| `focus_id`     | `String` (PK) | UUID                                                      |
| `topic`        | `String`      | Short label ("Railway deployment", "Discord integration") |
| `context_json` | `String`      | Relevant task IDs, file paths, decisions made             |
| `priority`     | `u8`          | 1-5, so Heiwa Agent knows what to foreground              |
| `created_at`   | `u64`         | When focus was established                                |
| `resolved_at`  | `u64`         | 0 = still active                                          |

### Reducers

- `insert_captain_message(message_id, session_id, role, content, timestamp, source)` — append raw message
- `mark_messages_compressed(session_id, before_timestamp)` — bulk mark after rolling compression
- `insert_captain_summary(summary_id, summary_type, content, range_start, range_end, messages_compressed)` — store compressed summary
- `upsert_captain_focus(focus_id, topic, context_json, priority)` — create or update focus
- `resolve_captain_focus(focus_id, resolved_at)` — mark focus as resolved
- `prune_captain_messages(before_timestamp)` — delete compressed messages older than `before_timestamp`. The reducer itself does NOT enforce the "only after digest exists" guard. Call-site in the Python memory loop must query `captain_summaries` for a `daily_digest` row covering `before_timestamp` before invoking this reducer.

---

## 2. Heiwa Agent Runtime

### Identity

The always-on orchestrator is renamed from `heiwa-captain` to `heiwa-agent`. This is the persistent identity — Heiwa's own voice. Displayed as "Heiwa Agent" in Discord, logs, and STDB agent_registry.

**Rename scope (implementation task):**

| What                     | Old                                                                            | New                                                                                                                         |
| ------------------------ | ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| Agent name (constructor) | `name="heiwa-captain"`                                                         | `name="heiwa-agent"`                                                                                                        |
| Class name               | `CaptainAgent`                                                                 | `HeiwaAgent`                                                                                                                |
| File name                | `agents/captain.py`                                                            | `agents/heiwa_agent.py`                                                                                                     |
| Logger                   | `logging.getLogger("Captain")`                                                 | `logging.getLogger("HeiwaAgent")`                                                                                           |
| Protocol subject         | `Subject.CAPTAIN_DM`                                                           | `Subject.HEIWA_AGENT_DM`                                                                                                    |
| Discord display          | "Agent Heiwa#3460"                                                             | Keep as-is (Discord bot name is configured in Discord Developer Portal, not code)                                           |
| STDB tables              | `captain_messages`, `captain_summaries`, `captain_focus`, `captain_directives` | Keep `captain_` prefix — renaming STDB tables requires module republish and is not worth the churn. The prefix is internal. |
| Imports in `main.py`     | `from heiwa_hub.agents.captain import CaptainAgent`                            | `from heiwa_hub.agents.heiwa_agent import HeiwaAgent`                                                                       |

### LLM Routing

| Mode    | Model                                                   | Rate Group                        | When                                                         |
| ------- | ------------------------------------------------------- | --------------------------------- | ------------------------------------------------------------ |
| Routine | `google-antigravity/gemini-3-flash`                     | `antigravity_flash` (20 turns/hr) | Status updates, simple observations, short replies           |
| Complex | Cascade (Antigravity Pro → Claude → Codex → Gemini CLI) | Various                           | Architecture discussions, strategy, detected high-complexity |

**Note:** `google-antigravity/gemini-3-flash` is a new model_tiers entry. The existing `antigravity/gemini-3-auto` (which maps to Gemini 3 Auto) is retired and replaced by the explicit flash/pro split. Model IDs use the `google-antigravity/` provider prefix to match the existing convention in `ai_router.json`.

**Complexity detection signals:** Long messages (>200 chars), architecture keywords ("design", "refactor", "deploy", "strategy"), question density, explicit operator requests ("think hard about this").

The `antigravity_flash` rate group is **exclusive to Heiwa Agent**. No ephemeral worker can consume it. This protects the conversation budget.

### Antigravity Rate Group Split

The existing `google_antigravity` rate group (35 turns/hr, one model) splits into:

| Rate Group          | Model                               | Turns/hr | Access              |
| ------------------- | ----------------------------------- | -------- | ------------------- |
| `antigravity_flash` | `google-antigravity/gemini-3-flash` | 20       | Heiwa Agent only    |
| `antigravity_pro`   | `google-antigravity/gemini-3.1-pro` | 15       | Cascade — any agent |

STDB changes: split the `google_antigravity` rate_group_state row into two. Replace the `antigravity/gemini-3-auto` model_tiers entry with two new entries using the `google-antigravity/` provider prefix. Update `ai_router.json` provider block and model registry accordingly.

### Memory Loop

Every message cycle:

```
1. RECEIVE  — Operator DMs or system event arrives
2. STORE    — Raw message → captain_messages (instant, no LLM)
3. LOAD     — Build active context window:
              a. Last N raw messages (within ~8K token budget)
              b. Active captain_focus entries
              c. System state snapshot (nodes, tasks, LLM health)
4. RETRIEVE — If topic seems new or references past work:
              Semantic search captain_summaries + knowledge_embeddings
              Inject top-K relevant summaries into context
5. REASON   — Send assembled context to google-antigravity/gemini-3-flash
              If complexity_signal > threshold → cascade to stronger model
6. RESPOND  — DM the operator, store agent response in captain_messages
7. FOCUS    — Update captain_focus if topic shifted or resolved
```

### Compression Triggers

| Trigger        | When                                                                                                                                                           | Action                                                                                                            |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| Rolling        | `captain_messages` WHERE `compressed = false` exceeds ~8K tokens (~32K chars, using `len(content) // 4` approximation counted in Python memory loop, not STDB) | Oldest uncompressed messages → LLM summary → `captain_summaries` (type=rolling), mark originals `compressed=true` |
| Daily digest   | Scheduled (midnight local or configurable)                                                                                                                     | All day's messages → LLM digest → `captain_summaries` (type=daily_digest), dump to maincloud                      |
| Boot hydration | On startup (after 15s delay for Discord)                                                                                                                       | Load last session's uncompressed messages + most recent 3 summaries into active window                            |

---

## 3. Agent Naming Convention

### Location-Aware Prefixes

| Location | Format                              | Examples                                                  |
| -------- | ----------------------------------- | --------------------------------------------------------- |
| Railway  | `heiwa-<role or provider>-<sector>` | `heiwa-agent`, `heiwa-claude-review`, `heiwa-codex-build` |
| MacBook  | `macbook-<provider>-<sector>`       | `macbook-ollama-shell`, `macbook-gemini-research`         |
| WSL      | `wsl-<provider>-<sector>`           | `wsl-antigravity-build`, `wsl-ollama-codegen`             |

The prefix is derived from `HEIWA_NODE_ID` (e.g., `macbook@heiwa-agile` → prefix `macbook`). Worker client strips the `@` suffix and uses the node name.

`heiwa-agent` is the only always-on agent. All others are ephemeral — they register in `agent_registry` on task claim and deregister on completion.

---

## 4. Railway Deployment — Cloud-First Stack

### Container Layout

```
start.sh
├── SpacetimeDB (127.0.0.1:3000, volume at /data/stdb)
├── Tailscale mesh (optional, for private networking)
├── Hub process (python -m apps.heiwa_hub.main)
│   ├── Heiwa Agent (always-on, DMs operator)
│   ├── Messenger (Discord bot)
│   ├── Spine (fleet orchestration)
│   ├── Executor (Railway-local task execution)
│   └── Telemetry (metrics collection)
├── HTTP API (:8080 → api.heiwa.ltd)
│   ├── POST /tasks — task ingress
│   ├── GET /health — STDB-aware health check
│   └── WebSocket endpoints
│       ├── /ws/worker — boost node registration + task delivery
│       ├── /ws/operator — operator event stream
│       └── /ws/tasks/{id} — per-task event stream
└── CLI tools (claude, gemini, codex — installed in container)
```

**STDB is internal only.** Port 3000 bound to 127.0.0.1, never exposed. All external access through hub API.

**Railway persistent volume** at `/data/stdb` — survives container restarts and redeployments.

**Maincloud sync:** Part of Heiwa Agent's maintenance tick (not a separate service). One-directional: Railway STDB → maincloud. Daily digests + knowledge embeddings. Maincloud is read-only archive for long-term recall and boost node hydration.

### Boost Node Connection

Boost nodes (MacBook, WSL, any device with the repo) connect via the existing `/ws/worker` WebSocket protocol:

```json
// 1. Register
{"type": "register", "worker_id": "macbook-devon", "auth_token": "...",
 "capabilities": {"ollama": true, "gpu": "m4_pro", "models": ["qwen3.5:4b", "glm-4.7-flash"]}}

// 2. Hub pushes task
{"type": "task_assignment", "data": {"task_id": "...", "intent": "build", ...}}

// 3. Worker reports result
{"type": "result", "task_id": "...", "status": "PASS", "summary": "..."}

// 4. Periodic heartbeat
{"type": "heartbeat"}
```

Boost nodes don't need their own STDB. They execute tasks locally and report results to the hub. Spine routes work to them when they're online and have matching capabilities.

### Environment Config

| Env               | Railway                  | MacBook (boost)         | WSL (boost)             |
| ----------------- | ------------------------ | ----------------------- | ----------------------- |
| `STDB_SERVER`     | `local` (in-container)   | N/A                     | N/A                     |
| `STDB_DATA_DIR`   | `/data/stdb` (volume)    | N/A                     | N/A                     |
| `HEIWA_NODE_TYPE` | `orchestrator`           | `boost`                 | `boost`                 |
| `HEIWA_NODE_ID`   | `railway@heiwa-cloud-hq` | `macbook@heiwa-agile`   | `wsl@heiwa-forge`       |
| `HEIWA_HUB_URL`   | `https://api.heiwa.ltd`  | `https://api.heiwa.ltd` | `https://api.heiwa.ltd` |

### Health Check Hardening

`/health` currently returns 200 unconditionally. Must verify STDB connectivity (`SELECT 1 FROM model_tiers LIMIT 1`) with a **2-second timeout** and return 503 if unreachable or timed out. A hung STDB counts as unreachable. Railway's restart policy (max 10 retries) handles recovery.

---

## 5. Error Handling

| Scenario                          | Response                                                                                                                                |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| STDB unreachable on boot          | Heiwa Agent starts degraded — no memory load, DMs "I'm online but lost my memory connection." Retries every 60s.                        |
| LLM call fails during compression | Raw messages stay uncompressed (`compressed=false` is default). Retry next tick.                                                        |
| Antigravity Flash rate exhausted  | Heiwa Agent goes quiet until cooldown. Does NOT cascade for routine reasoning — cascade only for complex operator conversations.        |
| Daily digest fails mid-generation | Partial digest discarded. Retried next maintenance tick. Raw messages never deleted until digest succeeds.                              |
| Boost node disconnects mid-task   | Spine detects WebSocket drop, marks task `FAIL`, Heiwa Agent DMs operator. Task retryable on Railway or another node.                   |
| Railway container restarts        | STDB volume persists. Heiwa Agent hydrates from `captain_messages` + latest summaries, DMs "Back online — picked up where we left off." |

---

## 6. Testing Strategy

| Test                         | Validates                                                                       |
| ---------------------------- | ------------------------------------------------------------------------------- |
| `test_captain_message_store` | Raw messages persist and retrieve from STDB                                     |
| `test_rolling_compression`   | Messages over token budget trigger summary, originals marked `compressed=true`  |
| `test_daily_digest`          | End-of-day digest covers all day's messages, stores in `captain_summaries`      |
| `test_boot_hydration`        | Fresh Heiwa Agent loads last session's uncompressed messages + recent summaries |
| `test_focus_tracking`        | Focus entries create, update priority, and resolve                              |
| `test_antigravity_isolation` | `antigravity_flash` rate group consumed only by Heiwa Agent, not cascade        |
| `test_complexity_escalation` | Complex operator messages trigger cascade instead of Flash                      |
| `test_agent_naming`          | Railway agents get `heiwa-` prefix, boost nodes get node-name prefix            |
| `test_health_stdb_ping`      | `/health` returns 503 when STDB is down                                         |
| `test_maincloud_dump`        | Daily digest syncs to maincloud on schedule                                     |

---

## 7. Out of Scope (YAGNI)

These are explicitly deferred. Build later if needed:

- Multi-operator DM support (only Devon for now)
- Heiwa Agent initiating tasks without operator input (proactive delegation)
- Real-time STDB replication Railway ↔ maincloud (one-way daily dump is enough)
- Boost node auto-discovery (nodes connect manually via WebSocket + auth)
- Conversation branching / threading within DMs (linear message stream)
- Voice/media in DMs (text only)

---

## 8. Future Integration Opportunities (TBB — To Be Built)

Open-source projects that provide immense value once Heiwa is fully operational. These are not dependencies — Heiwa works without them. They're force multipliers to evaluate when the core is stable.

### Memory & Retrieval

| Project                                             | Value to Heiwa                                                                                          | Integration Point                                                                                                                            |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| [Mem0](https://github.com/mem0ai/mem0)              | Structured long-term memory with automatic fact extraction, conflict resolution, and temporal awareness | Replace or augment the rolling compression LLM calls — Mem0 handles "what facts should I remember" better than raw summarization             |
| [LangMem](https://github.com/langchain-ai/langmem)  | Background memory management, semantic deduplication, memory consolidation during idle periods          | Could run as a background task in Heiwa Agent's maintenance tick, consolidating `captain_summaries` into higher-order memories               |
| [Letta (MemGPT)](https://github.com/letta-ai/letta) | Self-editing memory with explicit memory tiers (core, archival, recall) and memory pressure management  | Architectural inspiration for the tri-layer design. Could eventually replace the custom memory loop if Heiwa needs more sophisticated recall |

### Agent Orchestration

| Project                                                                                          | Value to Heiwa                                                                                   | Integration Point                                                                                                  |
| ------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| [CrewAI](https://github.com/crewAIInc/crewAI)                                                    | Multi-agent task decomposition with role-based delegation and structured output                  | Could enhance Spine's task routing — CrewAI's "crew" concept maps to Heiwa's ephemeral agent spawning              |
| [AutoGen](https://github.com/microsoft/autogen)                                                  | Conversational multi-agent patterns, human-in-loop protocols, code execution sandboxing          | AutoGen's group chat patterns could inform how Heiwa Agent coordinates multiple ephemeral workers on complex tasks |
| [Anthropic Claude Agent SDK](https://github.com/anthropics/claude-code/tree/main/packages/agent) | Official SDK for building custom Claude agents with tool use, guardrails, and structured outputs | Direct integration with `heiwa-claude-*` workers — use the SDK instead of raw CLI for more control                 |

### Execution & Sandboxing

| Project                                             | Value to Heiwa                                                                        | Integration Point                                                                                         |
| --------------------------------------------------- | ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| [E2B](https://github.com/e2b-dev/e2b)               | Cloud sandboxes for untrusted code execution (already in Heiwa's hard rules)          | Replace local subprocess execution for untrusted tasks — already architecturally planned                  |
| [Modal](https://github.com/modal-labs/modal-client) | Serverless GPU containers — spin up compute on demand without managing infrastructure | Future alternative to Ollama for GPU inference when local nodes are offline. Pay-per-use, no idle cost    |
| [Dagger](https://github.com/dagger/dagger)          | Programmable CI/CD pipelines as code — containerized build steps                      | Could replace or augment the GitHub Actions CI/CD pipeline with more portable, testable build definitions |

### Knowledge & Search

| Project                                               | Value to Heiwa                                                                    | Integration Point                                                                                                                          |
| ----------------------------------------------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| [Chroma](https://github.com/chroma-core/chroma)       | Purpose-built embedding database with metadata filtering and MMR search           | Could replace the brute-force cosine similarity in `MemoryService.query_knowledge()` — proper ANN index instead of scanning all embeddings |
| [Turbopuffer](https://turbopuffer.com/)               | Serverless vector database with zero cold starts                                  | Cloud-hosted alternative to local Chroma — pairs well with maincloud STDB for the semantic index layer                                     |
| [Docling](https://github.com/docling-project/docling) | Document parsing (PDF, DOCX, PPTX) into structured text with layout understanding | Feed into `MemoryService.index_file()` for richer knowledge extraction from non-code documents                                             |

### Observability & Monitoring

| Project                                                | Value to Heiwa                                                                          | Integration Point                                                                                                                      |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| [Langfuse](https://github.com/langfuse/langfuse)       | LLM observability — trace every prompt, token usage, latency, cost across all providers | Instrument all LLM calls (Heiwa Agent reasoning, ephemeral worker execution, compression) for debugging and cost tracking              |
| [Phoenix (Arize)](https://github.com/Arize-ai/phoenix) | LLM evals, tracing, and experiment tracking with a local-first UI                       | Alternative to Langfuse with stronger eval capabilities — useful for validating compression quality and intent classification accuracy |

### Communication & Control Plane

| Project                                                 | Value to Heiwa                                       | Integration Point                                                                                              |
| ------------------------------------------------------- | ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| [Matrix/Element](https://github.com/element-hq/synapse) | Self-hosted, federated messaging with E2E encryption | Future alternative to Discord for the operator control plane — fully sovereign, no third-party dependency      |
| [Ntfy](https://github.com/binwiederhier/ntfy)           | Simple HTTP-based push notifications to any device   | Lightweight alternative to Discord DMs for critical alerts when Discord is down or for devices without Discord |

---

## Design Decisions Log

| Decision                                           | Rationale                                                                                                             |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Per-message storage (not per-turn)                 | Heiwa Agent sends proactive messages that don't pair with operator input. Matches Discord's data model.               |
| Rolling + daily compression (both)                 | Rolling keeps the active window tight for fast responses. Daily digests build long-term institutional memory.         |
| Antigravity Flash for routine, cascade for complex | Protects conversation budget (20 turns/hr). Operator gets fast responses for status/chat, deep reasoning when needed. |
| STDB internal-only, never internet-facing          | Hub API is the single gateway. Boost nodes connect via WebSocket, not direct DB access.                               |
| Location-aware agent prefixes                      | `heiwa-` for Railway, `macbook-`/`wsl-` for boost nodes. Instantly identifies where work is executing.                |
| One-way maincloud sync                             | Railway → maincloud for archive. No bidirectional replication complexity. Maincloud is read-only.                     |
| Discord for cross-device conversation sync         | Already solved — DMs sync across all devices Devon is logged into. No custom sync needed.                             |
