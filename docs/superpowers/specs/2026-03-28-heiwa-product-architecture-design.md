# Heiwa Product Architecture Design

**Date:** 2026-03-28
**Status:** Historical design snapshot. Superseded by `HEIWA.md`,
`AGENTS.md`, and `docs/local-self-operation.md` for current product center,
naming, provider inventory, and local-first runtime truth.
**Supersedes:** `config/swarm/END_STATE_2026-03.md` on Captain model identity (no longer Gemini Flash-specific), ACP scope (deferred), and Skill execution engine (deferred). END_STATE should be updated to match this spec before implementation begins.

## Positioning

Heiwa is an autonomous agent operating system: a cloud control plane that coordinates the best coding agents and model providers across battlefields, with leases, policies, and persistent runtime state.

- vs Claude Code / Codex agent / Cursor: they are operators or clients; Heiwa is the always-on runtime above them.
- vs OpenRouter: they route inference; Heiwa routes inference plus execution, authority, state, and autonomy.
- vs OpenClaw: that becomes one subsystem inside Heiwa, not the product boundary.

## Enterprise

**Heiwa Limited** is the enterprise. All products, tools, and subsystems live under Heiwa Limited. "Heiwa Universe" is brand language for the collection — it does not appear in runtime specs or architecture docs.

## Core Architectural Decisions

### Hub-first, not Discord-first

Heiwa Hub is the product. Discord, Web, and CLI are equal clients over a generic event protocol (Wire). The Hub does not know or care which client is connected. If Discord goes down, the Hub keeps working.

The autonomy lives in the Captain (inside the Hub), not in any client surface. Clients render events and translate user input into Hub API calls. No client is privileged.

### Single-owner deployment

Each user deploys their own Heiwa Hub instance. There is no shared multi-tenant SaaS. Your Hub, your keys, your state, your Captain.

This does not mean there is only one actor inside the Hub. Discord identities, GitHub OAuth, future collaborators, and tool sessions can exist as internal principals. The instance belongs to one owner.

Identity has two layers:

- **`owner_id`** — the identity that owns the Hub instance. Scopes all tenant-level resources: Battlefields, Credentials, billing. One per instance in single-owner deployment.
- **`principal_id`** — the identity that performed an action. May be: the owner directly, a Discord identity, a GitHub OAuth collaborator, a tool session (Captain acting autonomously), or a future collaborator. Every write stamps both `owner_id` (who owns the record) and `principal_id` (who created it).

This separation means the audit trail can always answer "who owns this?" and "who did this?" independently. Read filtering uses `owner_id` (all records belong to the instance owner). Audit queries use `principal_id` (which actor created this Lease, approved this Mission, stored this credential).

If Heiwa Limited later offers managed hosting, `owner_id` becomes the tenant boundary with no schema migration.

### Captain is Heiwa

The Captain is not powered by one model. The Captain *is* Heiwa — the entire fleet. Its reasoning and execution draw from all available models across all providers, selected by the rate cascade. There is no "internal reasoning model" vs "execution model." The Captain picks the best available model with capacity for whatever it needs to do right now.

### Provider/model inventory

| Provider | Models | Rate Groups | Auth |
|----------|--------|-------------|------|
| Claude Code | Opus 4.6, Sonnet 4.6, Haiku 4.5/6 | 1 (Anthropic) | Claude Pro subscription |
| Codex | GPT-5.4, GPT-5.4-Mini | 1 (OpenAI) | ChatGPT Plus subscription |
| Gemini CLI | Gemini 3.1 Pro, Gemini 3 Flash | 1 (Google) | Google AI Pro subscription |
| Antigravity | Gemini 3.1 Pro (High), Gemini 3.1 Pro (Low), Gemini 3 Flash, Claude Opus 4.6, Sonnet 4.6 | 3 | Google AI Pro subscription |

Each model/provider/rate group is fully monitored by Pulse: rate limit state, request history, latency, cost, availability, capacity.

## Product Taxonomy

### Products

| Name | Type | What it is |
|------|------|-----------|
| **Heiwa Hub** | Core product | The autonomous agent OS. Railway-native control plane. Owns Captain, rate cascade, leases, events, provider monitoring. Each owner deploys their own instance. |
| **Heiwa CLI** | Client product | Local REPL and cockpit. `cd myproject && heiwa` boots a session, connects to Hub via Wire, registers a Battlefield. Primary operator surface. |
| **Heiwa Web** | Client product | Dashboard. Credential vault UI, session history, approval surface, Pulse monitoring. Equal client over Wire. |
| **Heiwa Trading** | Vertical product | Financial strategy / paper-trading vertical built on Hub. First proof that the platform supports domain-specific autonomous work. |

### Named subsystems (inside Hub)

| Name | What it is |
|------|-----------|
| **Captain** | The autonomous agent brain. Observes, decides, acts, communicates. Uses whatever model the cascade selects. Captain is Heiwa. Merges current Spine + HeiwaClaw. |
| **Vault** | BYOK credential store. Encrypted at rest. Trust boundary where owners deposit their API keys. |
| **Pulse** | Provider monitoring and telemetry. Rate limit state, health, cost metering, capacity across all rate groups. Absorbs current Telemetry agent. |
| **Wire** | The WebSocket event protocol between Hub and all clients. CLI, Web, Discord adapter — they all speak Wire. The public contract that makes Hub client-agnostic. |

### First-class concepts

| Name | What it is |
|------|-----------|
| **Node** | A reachable runtime endpoint. Railway container, MacBook boost node, future GPU boxes. Has health, heartbeat, and availability tracked by Pulse. Nodes are where execution physically happens — they are not providers or models. A Node hosts one or more Executors. |
| **Executor** | A provider/model surface available on a Node. "Claude Code on Railway", "Ollama qwen3.5 on MacBook", "Codex on Railway". Has a provider account (credentials in Vault) and rate group state (capacity, limits). The Captain's rate cascade selects an Executor, not a Node — the Node is the physical host, the Executor is the capability. |
| **Session** | The authenticated client transport context. Created when a client connects over Wire. Carries history, active tasks, and Battlefield context. Ephemeral — ends when the client disconnects or times out. Sessions do not own leases or authorization scope; they are an interaction channel, not an execution boundary. |
| **Mission** | The durable unit of work the Captain executes. Owns its own leases. May outlive or span Sessions — when a client disconnects, active Missions continue executing. Has steps, status, leases, and artifacts. The Captain is the execution authority, not the Session. |
| **Battlefield** | A workspace the Hub is focused on. Registered when CLI connects from a repo. Hub scopes context, tasks, file access, and history per Battlefield. An owner can have multiple Battlefields. See Battlefield data model below. |
| **Lease** | Permission grant with scope, chain state, and routing lock. The authority mechanism — nothing executes without one. |
| **Rate Cascade** | The routing algorithm. Spreads work across all available providers, cheapest-with-capacity first. Monitored by Pulse, executed by Captain. |

### Clients/adapters (not standalone products)

| Name | What it is |
|------|-----------|
| **Heiwa for Discord** | Adapter that renders Hub events as DMs/embeds/buttons. Translates Discord interactions into Hub API calls. Runs in the Hub container. |

### Internal infrastructure (no product names)

- SpacetimeDB — state layer inside Hub
- LocalBusTransport — in-process event bus
- OpenClaw — spawning mechanism for CLI tools; Captain uses OpenClaw to dispatch work to Executors (Claude Code, Codex, Gemini CLI) hosted on reachable Nodes. OpenClaw remains the execution gateway but is called by Captain, not exposed as a product boundary.
- Cognition pipeline (IntentNormalizer, RiskScorer, ComputeRouter, ProgramCompiler) — stays as a library that Captain calls. These are the Captain's decision-making internals, not separate agents.
- SDK/packages — shared libraries

## Hub API Surface

### Core resources

| Resource | What it represents |
|----------|--------------------|
| **Session** | Authenticated client transport context. Has history, active tasks, Battlefield context. Does not own leases — it is an interaction channel, not an execution boundary. |
| **Mission** | Durable work unit. Owns its leases. Steps, status, artifacts. May outlive or span Sessions. The Captain continues executing Missions after client disconnect. |
| **Event** | Something that happened — immutable, append-only. Task lifecycle, Captain observations, approvals, errors. |
| **Lease** | Permission grant with scope, chain state, routing lock. |
| **Credential** | Owner's BYOK keys. Encrypted at rest in Vault. |
| **Node** | Reachable runtime endpoint with health and heartbeat. |
| **Executor** | Provider/model surface on a Node with rate group state and credentials. |
| **Battlefield** | Workspace with scoped context and history. |

### Client protocol

- **Wire** (`/ws/client`) — clients connect, authenticate, receive a stream of Events. Primary channel.
  - **Auth handshake:** Client sends an auth token (OAuth bearer or session token) on connect. Hub validates and associates the connection with an owner identity. Unauthenticated connections are rejected.
  - **Delivery guarantee:** At-least-once. Events are persisted to STDB before emission. Clients must be idempotent on `event_id`.
  - **Catch-up:** On reconnect, client sends `last_seen_event_id`. Hub replays all Events after that ID from STDB. No bounded buffer — STDB is the replay source.
  - **Backpressure:** If a client falls too far behind (configurable threshold), the Hub closes the connection. Client reconnects and catches up from STDB.
- **REST endpoints** — imperative actions: create mission, approve/reject, upload credential, query history. Thin wrappers over STDB writes.
- **No client-specific logic in the Hub** — the Hub emits typed Events. Clients decide how to render them.

### Event schema

```
Event {
  event_id: str
  owner_id: str
  principal_id: str
  session_id: Option<str>
  mission_id: Option<str>
  battlefield_id: Option<str>
  event_type: enum (
    mission_created, mission_completed, mission_failed,
    task_started, task_completed,
    approval_needed, approval_resolved,
    captain_observation,
    lease_issued, lease_revoked,
    credential_stored,
    node_online, node_offline,
    ...
  )
  payload: dict
  timestamp: str
}
```

## Client Architecture

Clients are adapters, not agents. Each client:

1. Connects to Wire, authenticates with owner credentials
2. Subscribes to Events
3. Renders Events using the surface's native primitives
4. Translates user input into Hub REST calls

### What happens to current Messenger agent

Decomposed. Discord bot connection, slash commands, and message rendering move to the Heiwa for Discord adapter. Logic that decides what to say or when to act stays in the Hub (Captain). The adapter does not think — it receives events and formats them.

### Boot sequence

Hub boots: Captain, Pulse — the subsystems that do work. Clients connect after boot as external consumers over Wire. If a client surface is down, the Hub keeps working. Events queue in STDB. Client reconnects and catches up.

## Captain Architecture

### What merges

- `spine.py` (fleet orchestration, routing, assignment) + `heiwaclaw.py` (observation, execution, communication) merge into `captain.py`
- `heiwa_agent.py` and `executor.py` compatibility stubs are deleted

### Inner loop

The Captain is primarily event-driven (STDB subscriptions + local bus) with a periodic housekeeping tick for time-based concerns (lease expiry, heartbeat pruning, stale mission detection, rate limit resets). The tick interval is implementation-defined but expected ~10-30s.

```
event-driven path:
  receive event (STDB subscription or local bus)
  classify: is this something I need to act on?
  if yes:
    plan action (model selected by rate cascade)
    check: do I need approval? (risk score > threshold)
    if needs approval: emit approval_needed event, pause
    if approved or low-risk: execute via best available Executor on a reachable Node
    emit result events

periodic tick path:
  prune expired leases
  check node heartbeats, mark offline nodes
  detect stale missions, reassign or fail
  refresh rate limit state from Pulse
```

### What stays separate

- **Pulse** — monitoring is a distinct subsystem with its own state and tick
- **Client adapters** — external consumers over Wire

### Not in initial scope (deferred from END_STATE)

- ACP (agent-to-agent contracts) — Captain is the only autonomous agent initially. ACP matters when there are multiple Captains or delegated sub-agents. Deferred, not cancelled.
- Skill execution engine — YAML workflows are a later layer on top of the Captain's action loop. Deferred, not cancelled.
- HeiwaCells marketplace — catalog exists in `profiles.json`; installer/discovery UI is post-launch.

## Deployment Model

### Primary: Railway template

One-click deploy from Railway marketplace. Template provisions: Hub service, STDB instance, env var slots for API keys. Owner fills in keys, Hub boots, Captain starts. Under 5 minutes from zero to running.

### Secondary: Docker / self-host

`docker compose up` for owners who want to run on their own infra. Same image, different deployment target.

### Devon's instance

Tenant zero. No special-casing. Same template, same Hub, same Captain.

### Business model

- Open-source Hub
- Revenue (later): Railway template marketplace, managed hosting tier, premium integrations
- The ~$83 CAD/mo cost is the owner's own infrastructure — Heiwa Limited does not pay for user compute

## What This Design Kills

| Current name | Fate |
|-------------|------|
| Messenger agent | Becomes Heiwa for Discord client adapter |
| Spine agent | Absorbed into Captain |
| HeiwaClaw agent | Absorbed into Captain |
| Telemetry agent | Becomes Pulse subsystem |
| HeiwaBench | Becomes a Captain capability (self-evaluation) |
| HeiwaCells | Becomes Captain's persona/identity catalog |
| Multi-tenant user_id isolation | Reframed as ownership and audit scoping |
| Discord-first assumption | Hub-first, Discord is an equal client |

## Object Hierarchy

Owner deploys a Hub. Captain operates across Nodes via Executors. Work happens in Battlefields. Missions are durable work units that own their Leases. Sessions are client transport contexts. Events flow over Wire to clients. Pulse monitors Nodes (health) and Executors (capacity). Principals are attributed on every action.

## Battlefield Data Model

New STDB table. A Battlefield is a registered workspace.

```
Battlefield {
  battlefield_id: str (primary key, UUID)
  owner_id: str
  name: str (display name, e.g. "heiwa", "ai-dj")
  repo_url: Option<str> (git remote, if applicable)
  root_path: str (absolute path on the node that registered it)
  node_id: Option<str> (which Node the Battlefield is attached to — None if cloud-only)
  status: str (active, archived)
  created_at: str
  last_active_at: str
}
```

Lifecycle: CLI connects from a repo → registers or reattaches a Battlefield → Hub scopes Session context to it. Missions are created within a Battlefield. One Mission belongs to one Battlefield; one Battlefield has many Missions.

Archiving a Battlefield detaches it from active routing but preserves history.

## Node, Executor, and Provider Relationships

Three distinct concepts, layered:

- **Node** — a reachable runtime endpoint: Railway container, MacBook, future GPU box. Tracked in STDB `node_registry` table. Has health, heartbeat, platform, capabilities. Pulse monitors: is this Node reachable?
- **Executor** — a provider/model surface hosted on a Node: "Claude Code Opus 4.6 on Railway", "Ollama qwen3.5 on MacBook", "Antigravity Gemini 3.1 Pro (High) on Railway". An Executor combines a Node (where it runs) with a provider account (how it authenticates) and a rate group (what capacity it has). The Captain's rate cascade selects Executors.
- **Provider account** — an authenticated relationship with a provider: "my Claude Pro subscription", "my Google AI Pro account". Tracked in STDB `provider_accounts` table. Has credentials in Vault, rate group state in `rate_group_state` table.

Relationships:
- One Node hosts many Executors (Railway hosts Claude Code, Codex, Gemini CLI, Antigravity)
- One provider account backs many Executors (Google AI Pro backs Gemini CLI + Antigravity rate groups)
- Some Executors have no provider account (MacBook Ollama — local, no auth)

Pulse monitors both layers: Node-level health (is it reachable?) and Executor-level capacity (rate limits, token budget, model availability). The Captain queries Nodes for "can I reach this?" and Executors for "does this have capacity?"

## Ownership and Audit Enforcement

Every tenant-scoped record carries two identity fields:

- **`owner_id`** — who owns this record. Set from the authenticated instance owner. Used for read-time filtering (the Hub API only returns records matching the instance owner).
- **`principal_id`** — who created this record. Set from the acting principal: the owner themselves, a Discord identity, a collaborator, the Captain (autonomous action), or a tool session. Used for audit queries.

Enforcement:

1. **Write-time stamping** — every STDB write that creates a tenant-scoped record (Mission, Lease, Session, Battlefield, Credential, Event) stamps both `owner_id` and `principal_id` from the authenticated context. No record is created without both.
2. **Read-time filtering** — every query for tenant-scoped data includes `owner_id` in the filter. The Hub API never returns records belonging to a different owner.
3. **Audit querying** — `principal_id` enables answering "which Discord user triggered this Mission?", "did the Captain create this Lease autonomously or did the operator request it?", "which tool session stored this credential?"

In single-owner deployment, `owner_id` is the same for all records. `principal_id` is where the audit value lives.

The "observe to enforce" flip (step 8) refers specifically to Lease enforcement: in observe mode, execution proceeds even without a valid Lease (logging only). In enforce mode, execution is blocked without a valid Lease. Ownership stamping, principal attribution, and read filtering are always on.

## Critical Path (implementation order)

1. App-layer ownership and audit scoping (migrate `user_id` → `owner_id` + `principal_id`)
2. BYOK Vault routing integration (per-owner credentials feed the rate cascade) — *parallelizable with step 3*
3. Wire protocol (WebSocket client endpoint, event schema, catch-up) — *parallelizable with step 2*
4. Captain merge (Spine + HeiwaClaw into captain.py) — *depends on step 3 for event delivery testing*
5. Pulse subsystem (absorb Telemetry, per-model/provider/rate-group monitoring)
6. Heiwa CLI battlefield registration and Wire client — *primary operator surface, built before Discord adapter*
7. Heiwa for Discord adapter (decompose Messenger) — *parallelizable with step 6*
8. Scope enforcement flip (observe to enforce mode)
9. Railway template packaging

## Known Implementation Gaps

These are not design issues — they are existing schema gaps that the critical path will resolve:

- **Existing `user_id` columns need migration to `owner_id` + `principal_id`** — all tenant-scoped tables currently have `user_id`. Step 1 splits this into `owner_id` (ownership/filtering) and `principal_id` (audit attribution). `CapabilityLease` additionally needs both fields added (currently has neither).
- **Dual node tables** — STDB has both `node_registry` (NodeRegistryEntry) and `nodes` (NodeStatus) with overlapping fields. One should be consolidated or killed before step 4 (Captain merge).
- **`events` table does not exist** — the Wire catch-up mechanism requires a new STDB table. Step 3 creates it.
- **`battlefield` table does not exist** — new STDB table per the Battlefield Data Model section. Step 6 (CLI battlefield registration) creates it.
- **Battlefield `root_path` is node-specific** — the same repo accessed from multiple nodes (MacBook and Railway) will have different root paths. The `node_id` field on Battlefield handles this, but implementations should treat (repo_url, node_id) as the dedup key, not root_path alone.

## Companion Document Updates

When implementation begins, the following existing docs must be updated to match this spec:

- `config/swarm/END_STATE_2026-03.md` — Captain is no longer Gemini Flash-specific; ACP and Skill engine are deferred
- `ops/context/HEIWA.md` — hard rules reference Captain instead of HeiwaClaw; task routing table reflects Railway-primary (macbook is not default for build/fix/review)
- `CLAUDE.md` — agent inventory section reflects Captain + Pulse, not Spine + HeiwaClaw + Telemetry + Messenger
