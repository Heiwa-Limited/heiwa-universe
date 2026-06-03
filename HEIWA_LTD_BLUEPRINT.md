# Heiwa Ltd — Programmatic Value Report & 24/7 Server Blueprint

Author: Claude (claude@heiwa.ltd) for Devon (devon@heiwa.ltd)
Date: 2026-06-01
Status: Strategic blueprint. Grounded in a cold review of `dmac.local` on 2026-06-01.
Scope: (1) technical value assessment of this machine, (2) real revenue-workflow research, (3) conceptual full-stack chart for Heiwa Limited, (4) the `*@heiwa.ltd` agent-identity scheme and recommended tooling.

---

## 0. Executive Truth

**This machine is already a partial 24/7 personal-AI server. The build is not greenfield — it is consolidation.**

At review time, three Heiwa daemons were live under launchd (`ltd.heiwa.app` on `127.0.0.1:7474`, `com.heiwa.orchestrator` as a `KeepAlive` Node daemon, `com.heiwa.ollama`), plus two mac-agent jobs (`cockpit`, `market-supervisor`). The runtime principal is already `devon@heiwa.ltd`. Ollama serves four local models. Five providers are registered and reachable. A 17-crate Rust workspace implements routing (DREX), quota ledgers, receipts, an OS-keychain vault, an MCP scaffold, and **A2A-compatible agent envelopes** (`heiwa_a2a`). A physical/digital sync read-model already exists as `heiwa life`.

The gap between "what runs today" and "Devon's 24/7 assistant that syncs physical and digital life" is **integration and identity**, not invention. The single highest-leverage move is to promote every actor — human, provider, agent, tool — into one addressable `*@heiwa.ltd` identity namespace with a credential, a quota row, and a receipt trail. That idea is already shipping in the market (Read AI's `ada@read.ai`), already half-built here (`devon@heiwa.ltd` + `heiwa_a2a`), and maps onto the converging 2026 agent-identity standard (A2A Agent Cards + OAuth/OIDC/mTLS).

---

## Part 1 — Technical Report: Programmatic Value on `dmac.local`

### 1.1 Hardware & host

| Property | Value | Implication |
| --- | --- | --- |
| Machine | Apple M4 Pro, 12 cores, 24 GB unified memory | Comfortably runs 2 concurrent local models + daemons + dev |
| OS | macOS 26.5 (build 25F71) | Modern launchd, full computer-use surface |
| Disk | 460 GB volume, **226 GB free** | Room for more local models, vector stores, session/evidence DBs |
| Uptime | 3 days, load avg ~4.0 | Already operated as an always-on box |
| Toolchains | rustc 1.95, Node 26.0, Python 3.14.5 | All three runtimes the stack needs are current |

**Assessment:** This is a capable always-on edge server. 24 GB is the real ceiling — it caps you at ~2 warm local models and forces cloud fallback for class-5 reasoning. That is fine: the cost-first router is designed around exactly this constraint.

### 1.2 Heiwa runtime — what is actually installed and running

`~/.heiwa/` is a populated runtime state root, not a stub:

- **`bin/heiwa`** (15 MB release binary) — full CLI surface:
  `install · login · doctor · register · receipts · devices · auth · providers · models · life · app · workers · approvals · mail · route · session · loop · shell`
- **`bin/heiwa-route`** — live inference router (`heiwa-route status` returns real routing decisions).
- **`identity.json`** → `user_id: devon-canonical`, `email: devon@heiwa.ltd`. **The human principal already uses the target naming scheme.**
- **`machine.json`** → device UUID, host `dmac.local`, runtime inventory, `installed_at 2026-04-05`.
- **`accounts.json`** → 5 provider accounts with per-model capability classes (1–5), context windows, and per-1k cost — a real model inventory, not a hardcoded list.
- **`state/`** → `dispatch/ events/ evidence/ goals/ health/ inventory/ schedulers/ sessions.sqlite3 workers.json` — an operating control plane with a live worker registry (`workers.json`, freshly written at review time).
- **`connection.json`** → SpacetimeDB cloud target `heiwaproductiondb @ maincloud.spacetimedb.com` (evidence/adjudication plane, token currently empty = local-only mode).

**Live services (launchd):**

| Job | What it is | Status |
| --- | --- | --- |
| `ltd.heiwa.app` | Heiwa.app runtime, listening `127.0.0.1:7474` | RUNNING |
| `com.heiwa.orchestrator` | `node ~/.heiwa/orchestrator/daemon.js`, `RunAtLoad` + `KeepAlive` | RUNNING |
| `com.heiwa.ollama` | Local inference keep-alive | RUNNING |
| `com.heiwa.mac-agent.cockpit` | Operator cockpit agent | installed |
| `com.heiwa.mac-agent.market-supervisor` | Trading supervisor | installed |

**This is the 24/7 server you asked to design — it is already breathing.** The orchestrator daemon with `KeepAlive` is the unattended loop; the missing pieces are durable job semantics, richer intake feeds, and the identity layer.

### 1.3 The Rust workspace — the engine

9 apps + 17 crates under one Cargo workspace (`github.com/Strategizing/heiwa-universe`, Apache-2.0). The crates that matter for a 24/7 assistant:

| Crate | Role | Why it matters here |
| --- | --- | --- |
| `heiwa_drex` | Routing kernel — binds provider + quota + vault | The brain that picks model/route per task |
| `heiwa_provider` | Provider adapters | Wraps Claude/Codex/Gemini/Ollama without owning their internals |
| `heiwa_quota` | **SQLite quota ledger** | Cost-first routing has a real accounting substrate |
| `heiwa_receipts` | **SQLite receipt store** | Evidence plane — every action can be proven |
| `heiwa_vault` | **OS keychain wrapper** | Credentials live in Keychain, not flat files |
| `heiwa_a2a` | **A2A-compatible agent envelopes** | Multi-agent identity/messaging is already protocol-shaped |
| `heiwa_mcp` | MCP scaffold | Tool/connector fabric |
| `heiwa_session` / `heiwa_loop` / `heiwa_repl` | Session + bounded execution loop + REPL | The conversation/execution spine |
| `heiwa_memory` / `heiwa_embed` | Memory + embeddings | Long-term context (pairs with `qwen3-embedding`) |
| `heiwa_stdb` | SpacetimeDB client | Evidence sync when online |

Apps include `heiwa_shell` (primary operator surface), `heiwa_core` (DREX kernel), `heiwa_orchestrator`, `heiwa_app`, **`heiwa_limbs`** (the actuator/connector layer — the "limbs" that act on the world), plus `heiwa_trading`, `heiwa_dj`, `heiwa_cli`, and quarantined `heiwa_hub`.

**Assessment:** The hard, unglamorous primitives a trustworthy 24/7 agent needs — routing, quota accounting, receipts, keychain secrets, agent-to-agent envelopes — **already exist as code.** Most "AI assistant" projects never build these. This is the moat.

### 1.4 The physical↔digital sync surface already exists

`heiwa life` is the read-model that fuses life signal:

```
heiwa life status | today | freshness | approvals
heiwa life import home | claude | codex | calendar  (--dry-run, write-gated)
```

This is the literal seed of "sync the user's physical world and digital world." It already imports calendar + home + provider-session context into one read-model and is deliberately **write-gated** ("Writes nothing unless explicit STDB sync is added later"). That is the correct safety posture for an always-on agent.

### 1.5 Connected capability surface (the "limbs" available today)

Beyond the repo, this machine is wired into an unusually deep tool surface. Observed connected/available MCP + native capability this session:

- **Communication:** Gmail (search/draft/label/threads), Google Calendar (events/suggest-time/respond), iMessage (read/search/send), Apple Notes (read/write).
- **Files/knowledge:** Google Drive (search/read/create), Notion (HTTP MCP), local filesystem.
- **Actuation:** computer-use (screenshot + mouse/keyboard control of native apps), Claude-in-Chrome + Control-Chrome (DOM-aware browser control), Playwright MCP.
- **Ops/dev:** Docker MCP gateway, git MCP, GitHub, Railway (`~/.railway`), Cloudflare Wrangler (`~/.wrangler`), Doppler secrets (`~/.doppler`), SpacetimeDB client creds.
- **Scheduling:** `scheduled-tasks` MCP + native cron/launchd.
- **Domain:** Kindora (foundation/grants 990 data), Figma, plus dozens of enterprise plugin connectors (Slack, HubSpot, Linear, Asana, Atlassian, Stripe-adjacent sales tools, etc.) available via OAuth.

**Assessment:** The intake + actuation breadth is already enterprise-grade. The constraint is not "what can it touch" but "how is that access governed, identified, and proven." Again: identity + governance, not capability.

### 1.6 Latent / redundant assets (cleanup signal)

Home holds many overlapping agent experiments: `.augment`, `.cagent`, `.openclaw`, `.openclaw-heiwa-antigravity`, `.picoclaw`, `.mac-agent`, `.agents`, `.antigravity`. These are programmatic value as *reference*, but they are also fragmentation. A 24/7 product wants **one** orchestrator of record (Heiwa) with the rest archived. Recommend consolidating into `heiwa_archive/` and letting `heiwa_limbs` + the orchestrator be the single actuation path.

### 1.7 Value verdict

| Dimension | Rating | Note |
| --- | --- | --- |
| Always-on host readiness | ★★★★☆ | Running 24/7; 24 GB RAM is the ceiling |
| Local inference | ★★★★☆ | 4 models warm; cost-first router live |
| Core agent primitives (routing/quota/receipts/vault/A2A) | ★★★★★ | Rare and already built |
| Intake breadth (mail/cal/msg/files/browser) | ★★★★★ | Enterprise-grade surface wired |
| Identity & governance | ★★☆☆☆ | **The gap. The product unlock.** |
| Durable orchestration | ★★★☆☆ | KeepAlive daemon exists; no journal/retry semantics yet |
| Productization (one coherent app) | ★★★☆☆ | `Heiwa.app` + shell exist; fragmentation remains |

---

## Part 2 — Market Research: Real Revenue/Income via Digital Workflows

Everything below is sourced. The pattern across all of it: **the money is in time-saving + revenue-generating + compliance/evidence workflows run continuously with minimal human lift.**

### 2.1 The solo-operator economics are real and large

- A typical 2026 solo-founder stack costs **$300–500/mo** (AI coding, content, automation, support) and replaces a team that would cost **$80k–120k/mo**. ([Taskade](https://www.taskade.com/blog/one-person-companies), [Fortune](https://fortune.com/2026/05/18/solo-founders-ai-automation-entire-teams-entrepreneurs/))
- Solopreneurs using AI agents reported **average revenue +340%** with no increase in hours; top performers grow revenue 2.3× peers and cut admin time up to 80%. ([selfemployed.com](https://www.selfemployed.com/news/ai-agents-for-solopreneurs-2026/))
- Existence proofs: **HeadshotPro $3.6M ARR** as a solo op; **Base44** hit 250k users + profitability in 6 months, sold to Wix for **$80M**. ([GreyJournal](https://greyjournal.net/hustle/grow/solo-founders-million-dollar-ai-businesses-2026/))

### 2.2 The repeatable money-making workflows (run-able by a 24/7 agent)

From the demand-side surveys, the workflows businesses actually pay for, ranked by how cleanly a 24/7 agent server can run them:

| Workflow | What the agent does unattended | Revenue model | Source |
| --- | --- | --- | --- |
| **AI automation agency** | Build + operate workflow automations for clients | Retainer; ~$40k/mo from 5 clients @ $5k, ~85% margin | [Medium/AI Studio](https://medium.com/the-ai-studio/how-ai-agencies-are-really-making-money-in-2026-6ab696804300), [Dan Martell](https://www.danmartell.com/the-most-in-demand-ai-services-businesses-will-pay-for-in-2026/) |
| **AI SDR / lead gen** | Prospect 24/7 on real-time signals, enrich, re-engage, multichannel outreach | Per-seat or performance; 20% lower CPL, +10% conversion observed | [Warmly](https://www.warmly.ai/p/blog/agentic-ai-examples), [monday](https://monday.com/blog/crm-and-sales/ai-lead-generation-software/) |
| **Content repurposing** | Long-form → platform-specific video/social/written assets | Productized service; "least upfront investment" | [LeadsBuddha](https://leadsbuddha.com/blog/ai-agents-replacing-marketing-workflows-2026) |
| **Marketing ops automation** | Campaign build, optimization, reporting | Retainer; 73% faster campaigns, 68–80% shorter content cycles | [thesmarketers](https://thesmarketers.com/blogs/ai-agentic-workflows-marketing/) |
| **Micro-SaaS / digital product** | A single sharp workflow wrapped as a paid product | Subscription/usage | [GreyJournal](https://greyjournal.net/hustle/grow/solo-founders-million-dollar-ai-businesses-2026/) |
| **Research / due-diligence-as-a-service** | Continuous structured research with receipts | Per-report or retainer | [Warmly](https://www.warmly.ai/p/blog/agentic-ai-examples) |

> Demand quote: *"Services tied to time savings, revenue generation, and compliance will attract the largest budgets."* — [Dan Martell](https://www.danmartell.com/the-most-in-demand-ai-services-businesses-will-pay-for-in-2026/). Heiwa's receipts/evidence plane is a direct fit for the "compliance" budget.

### 2.3 The product category Heiwa App sits in is validated and live

- **Read AI "Ada"** — an email-native **digital twin**: you cc `ada@read.ai` on any thread and it schedules, drafts replies, and answers from your meetings/email/files/CRM (20+ integrations, ~10k docs/user; Slack/Teams next). ([TechCrunch](https://techcrunch.com/2026/02/26/read-ai-launches-an-email-based-digital-twin-to-help-you-with-schedules-and-answers/), [GeekWire](https://www.geekwire.com/2026/read-ai-rolls-out-digital-twin-that-can-respond-to-work-emails-and-schedule-meetings/))
- **"AI Twin"** — a **privacy-first Personal OS**, platform-agnostic, spanning life-admin → fitness → interview prep. ([Newsfile](https://www.newsfilecorp.com/release/283445/AI-Twin-Introduces-Personalized-Digital-Assistant-for-Smarter-Living-and-Enhanced-Productivity))

**Strategic read:** `ada@read.ai` is exactly the `claude@heiwa.ltd` pattern — an agent reachable as an email address. Read AI is cloud-multi-tenant; **Heiwa's wedge is local-first + sovereign + receipt-backed.** "AI Twin" markets "privacy-first Personal OS" — that is Heiwa's positioning, and Heiwa actually *runs on your hardware* rather than promising privacy in someone else's cloud.

### 2.4 The modern tooling to build on (the "new tooling" ask)

| Layer | 2026 best-in-class | Self-host? | Fit for Heiwa |
| --- | --- | --- | --- |
| **Durable orchestration** | **Restate** (single binary, journaled durable execution, crash-resume) | ✅ yes | Replace/back the KeepAlive Node daemon with real retry/journal semantics ([Restate](https://docs.restate.dev/ai/patterns/durable-agents)) |
| | Temporal (high-throughput, 1000s wf/s) | ✅ heavy (4 containers, 4GB) | Overkill locally; reserve for hosted plane ([earezki](https://earezki.com/ai-news/2026-03-12-temporal-vs-n8n-which-should-you-self-host/)) |
| **Visual workflow glue** | **n8n** — native MCP (`MCP Server Trigger` + `MCP Client Tool`); expose any workflow as a tool Claude/GPT can call | ✅ $5 VPS / local | Non-code automations + connector breadth ([n8n docs](https://docs.n8n.io/hosting/starter-kits/ai-starter-kit/), [automationbyexperts](https://automationbyexperts.com/blog/n8n-ai-workflow-automation-guide-2026)) |
| **TS agent framework** | **Mastra 1.0** (Jan 2026) — agents + workflows + RAG + memory + MCP; Cloudflare Durable Objects storage adapter; used by Replit/PayPal/Brex | ✅ yes | If/when a TS surface is wanted beside the Rust core ([generative.inc](https://www.generative.inc/mastra-ai-the-complete-guide-to-the-typescript-agent-framework-2026)) |
| **Browser actuation** | **Stagehand v3** — `act/extract/observe` + Agent; **REST API** callable from Claude Code/MCP/n8n/curl; CDP-direct, 44% faster | ✅ OSS | The `browser@heiwa.ltd` limb ([Browserbase](https://www.browserbase.com/blog/stagehand-v3)) |
| **Managed browser runtime** | **Browserbase** — cloud browsers w/ **Agent Identity**, session replay, captcha, zero-infra Functions | cloud | Off-machine browser jobs + the "(browser)base" you named ([Browserbase](https://www.browserbase.com/stagehand)) |
| **Hosted edge plane** | **Cloudflare Agents SDK + Workflows + Durable Objects + Sandbox** | cloud (you have `wrangler`) | The multi-tenant plane when Heiwa Ltd sells to others ([Cloudflare](https://developers.cloudflare.com/workflows/get-started/durable-agents/)) |
| **Agent identity/interop** | **A2A** (Agent Cards: JSON capabilities + OAuth2/OIDC/mTLS; 150+ orgs, Linux Foundation) + **MCP** (tools) | open standard | Formalizes `*@heiwa.ltd` registry; `heiwa_a2a` already aligned ([a2a-protocol.org](https://a2a-protocol.org/v0.2.5/specification/), [fluxa](https://fluxapay.xyz/learning/how-ai-agents-authenticate-across-platforms-2026)) |
| **Local inference** | **Ollama** (running) + Open WebUI optional | ✅ local | Already the cost-first floor |

---

## Part 3 — Conceptual Chart: This Machine as Devon's 24/7 Heiwa Server

### 3.1 The identity namespace — `*@heiwa.ltd` (the unlock)

Every actor becomes one addressable principal with an A2A Agent Card, a vault credential, a quota-ledger row, and a receipt trail. This is the concrete realization of your request.

| Address | Type | Backing credential (vault) | Rate group | Role in the 24/7 loop |
| --- | --- | --- | --- | --- |
| **devon@heiwa.ltd** | human / owner | device key (exists today) | — | Principal. Final approver of gated actions. |
| **claude@heiwa.ltd** | provider-agent | OAuth CLI `claude` | anthropic | Reasoning, review, code, writing |
| **codex@heiwa.ltd** | provider-agent | OAuth CLI `codex` | openai | Code, alternate reasoning |
| **gemini@heiwa.ltd** | provider-agent | OAuth CLI `gemini` | google | Long-context, research, reasoning |
| **antigravity@heiwa.ltd** | provider-agent | OAuth (bonus quota) | google_bonus | Overflow reasoning |
| **ollama@heiwa.ltd** | local-runtime | localhost (no secret) | local | Always-on cheap/private floor |
| **orchestrator@heiwa.ltd** | system-agent | device key | — | The 24/7 scheduler + durable loop |
| **cron@heiwa.ltd** | system-agent | device key | — | Time-triggered jobs (digests, sweeps) |
| **mail@heiwa.ltd** | tool-limb | Gmail OAuth | — | Read/draft/send/triage email |
| **calendar@heiwa.ltd** | tool-limb | Google Calendar OAuth | — | Scheduling, conflict resolution |
| **messages@heiwa.ltd** | tool-limb | iMessage bridge | — | Read/send messages |
| **browser@heiwa.ltd** | tool-limb | Stagehand/Browserbase key | — | Web research + actuation |
| **files@heiwa.ltd** | tool-limb | Drive/Notes/Notion OAuth | — | Documents, knowledge |
| **trader@heiwa.ltd** | domain-agent | scoped key | — | Market-supervisor (paper first) |

Routing decisions, costs, and approvals now read in plain language: *"claude@heiwa.ltd drafted a reply; mail@heiwa.ltd staged it; devon@heiwa.ltd must approve send."* Each line is a receipt.

### 3.2 The layered architecture (Intake → Identity/Route → Execution → Evidence → Surface)

```
                          DEVON  (devon@heiwa.ltd)
                 one conversation · approvals · corrections
                                   │
 ┌─────────────────────────────────┴──────────────────────────────────┐
 │  SURFACE                                                            │
 │  Heiwa.app (127.0.0.1:7474) · heiwa shell REPL · web cockpit        │
 │  email twin (devon@heiwa.ltd) · mobile · iMessage in/out           │
 └─────────────────────────────────┬──────────────────────────────────┘
                                   │
 ┌─────────────────────────────────┴──────────────────────────────────┐
 │  INTAKE  (senses — physical + digital)        [heiwa life import]   │
 │  mail · calendar · iMessage · Notes · Drive/Notion · browser ·      │
 │  GitHub · files · health/home signals · runtime alerts             │
 └─────────────────────────────────┬──────────────────────────────────┘
                                   │  intent + passive feeds
 ┌─────────────────────────────────┴──────────────────────────────────┐
 │  IDENTITY & ROUTING   (DREX kernel)                                 │
 │  *@heiwa.ltd registry (A2A Agent Cards)                             │
 │  heiwa_vault (Keychain) · heiwa_quota (SQLite ledger)              │
 │  cost-first router:  local ▸ free-tier ▸ subscription              │
 └─────────────────────────────────┬──────────────────────────────────┘
                                   │  routed, budgeted, identified work
 ┌─────────────────────────────────┴──────────────────────────────────┐
 │  EXECUTION   (agents · limbs · workers)                            │
 │                                                                    │
 │  AGENTS:  ollama@ (local)  claude@  codex@  gemini@  antigravity@  │
 │  LIMBS (heiwa_limbs):  mail@ calendar@ messages@ browser@ files@   │
 │           ↳ browser@ = Stagehand v3 / Browserbase / Chrome MCP     │
 │  DURABLE LOOP:  orchestrator@ + cron@                              │
 │           ↳ today: KeepAlive Node daemon                           │
 │           ↳ upgrade: Restate journal (retry/resume) + n8n glue     │
 │  SAFE work runs in background · RISKY writes → staged actions      │
 └─────────────────────────────────┬──────────────────────────────────┘
                                   │  every read/action emits proof
 ┌─────────────────────────────────┴──────────────────────────────────┐
 │  EVIDENCE   (proof plane)                                          │
 │  heiwa_receipts (SQLite, source-linked) · approvals packets ·      │
 │  audit logs (~/.heiwa/logs) · STDB sync → heiwaproductiondb        │
 │  (local-first now; cloud adjudication when online)                 │
 └────────────────────────────────────────────────────────────────────┘

 RUNS 24/7 UNDER launchd:  ltd.heiwa.app · com.heiwa.orchestrator · com.heiwa.ollama
```

### 3.3 The day-in-the-life loop (how physical/digital sync feels)

```
  06:30  cron@ wakes → mail@/calendar@/messages@ pull overnight signal
         → ollama@ summarizes cheap/local → "morning brief" to devon@
  daytime passive: intake watches inbox/cal/threads; safe reads auto-run,
         each emitting a receipt; risky sends staged for one-tap approval
  on-demand: devon@ asks in one thread → DREX routes
         (local first, claude@/gemini@ for hard reasoning) → limbs act
  background jobs: browser@ runs research/lead-gen/repurposing on schedule
         → drafts land in files@ + a digest, gated before anything ships
  evening: cron@ compiles "what changed / what I did / what needs you /
         what's proven" → devon@ ; STDB sync when online
```

### 3.4 The revenue overlay (machine → income)

```
   24/7 SERVER CAPABILITY            →   PRODUCTIZED AS                →  REVENUE
 ─────────────────────────────────────────────────────────────────────────────
  browser@ + cron@ unattended        →  content-repurposing service   →  productized retainer
  mail@ + lead enrichment + outreach →  AI SDR / lead-gen-as-a-service →  perf + retainer
  orchestrator@ + n8n workflows      →  AI automation agency (5 clients)→ ~$40k/mo @ ~85% margin
  receipts/evidence plane            →  compliance-grade "agent w/ proof"→ premium tier
  the whole stack, packaged          →  Heiwa App = sovereign Personal OS→ subscription/usage
```

---

## Part 4 — Build Sequence for Heiwa Limited

Ordered by leverage. Each step is grounded in something already on the machine.

1. **Ship the identity namespace.** Promote `accounts.json` account-ids → `*@heiwa.ltd` principals; emit an A2A Agent Card per principal (`heiwa_a2a` already speaks the envelope). Surface in `heiwa auth status` / `heiwa providers`. *Unlock: every receipt, route, and approval becomes legible.*
2. **Harden the durable loop.** Put `Restate` (single self-hosted binary) under `orchestrator@`/`cron@` for journaled retry/resume; keep launchd as the supervisor. *Unlock: jobs survive crashes — real 24/7.*
3. **Light up intake feeds.** Turn `heiwa life import` from `--dry-run` read-model into governed passive feeds for `mail@`/`calendar@`/`messages@`. *Unlock: the assistant notices things you didn't ask about.*
4. **Stand up `browser@`.** Wire Stagehand v3 (REST API) + optional Browserbase behind `heiwa_limbs`. *Unlock: unattended web work = the first revenue workflow.*
5. **Pick one revenue workflow and run it for yourself first.** Recommend content-repurposing or research-with-receipts — lowest infra, uses `browser@` + evidence plane. Prove it on Devon's own output before selling it.
6. **Consolidate the agent-experiment sprawl** (`.openclaw`, `.cagent`, `.augment`, `.picoclaw`, `.mac-agent`, …) into `heiwa_archive/`. One orchestrator of record.
7. **Defer the cloud plane.** Cloudflare Agents/Workflows/DO + STDB sync only when Heiwa Ltd sells to others. Local-first stays the wedge and the privacy story.

---

## Appendix A — Evidence (what was directly observed on 2026-06-01)

- `sysctl` / `sw_vers`: M4 Pro, 12 cores, 24 GB, macOS 26.5, 226 GB free, uptime 3d.
- `launchctl list | grep heiwa`: `ltd.heiwa.app`, `com.heiwa.orchestrator`, `com.heiwa.ollama` running; cockpit + market-supervisor plists present.
- `lsof -iTCP -sTCP:LISTEN`: `heiwa` on `127.0.0.1:7474`, `ollama` on `127.0.0.1:11434`.
- `heiwa-route status`: Ollama v0.24.0 + 4 models; 5 provider accounts connected; cost-first routing (`code→ollama`, `chat→ollama`, `reason→gemini`, `review→claude`).
- `~/.heiwa/identity.json`: `email: devon@heiwa.ltd`.
- `~/.heiwa/connection.json`: STDB `heiwaproductiondb @ maincloud.spacetimedb.com` (token empty = local mode).
- `Cargo.toml`: 9 apps + 17 crates incl. `heiwa_a2a`, `heiwa_drex`, `heiwa_quota`, `heiwa_receipts`, `heiwa_vault`, `heiwa_mcp`, `heiwa_limbs`.
- `heiwa life --help`: physical/digital sync read-model, write-gated.

## Appendix B — Sources

Solo/revenue: [Fortune](https://fortune.com/2026/05/18/solo-founders-ai-automation-entire-teams-entrepreneurs/) · [Taskade](https://www.taskade.com/blog/one-person-companies) · [GreyJournal](https://greyjournal.net/hustle/grow/solo-founders-million-dollar-ai-businesses-2026/) · [selfemployed.com](https://www.selfemployed.com/news/ai-agents-for-solopreneurs-2026/) · [Dan Martell](https://www.danmartell.com/the-most-in-demand-ai-services-businesses-will-pay-for-in-2026/) · [Medium/AI Studio](https://medium.com/the-ai-studio/how-ai-agencies-are-really-making-money-in-2026-6ab696804300) · [Warmly](https://www.warmly.ai/p/blog/agentic-ai-examples) · [monday](https://monday.com/blog/crm-and-sales/ai-lead-generation-software/) · [LeadsBuddha](https://leadsbuddha.com/blog/ai-agents-replacing-marketing-workflows-2026) · [thesmarketers](https://thesmarketers.com/blogs/ai-agentic-workflows-marketing/)

Digital twin: [TechCrunch](https://techcrunch.com/2026/02/26/read-ai-launches-an-email-based-digital-twin-to-help-you-with-schedules-and-answers/) · [GeekWire](https://www.geekwire.com/2026/read-ai-rolls-out-digital-twin-that-can-respond-to-work-emails-and-schedule-meetings/) · [Read AI](https://www.read.ai/digital-twin) · [AI Twin](https://www.newsfilecorp.com/release/283445/AI-Twin-Introduces-Personalized-Digital-Assistant-for-Smarter-Living-and-Enhanced-Productivity)

Tooling: [Restate](https://docs.restate.dev/ai/patterns/durable-agents) · [Temporal vs n8n](https://earezki.com/ai-news/2026-03-12-temporal-vs-n8n-which-should-you-self-host/) · [n8n AI starter kit](https://docs.n8n.io/hosting/starter-kits/ai-starter-kit/) · [n8n 2026 guide](https://automationbyexperts.com/blog/n8n-ai-workflow-automation-guide-2026) · [Mastra guide](https://www.generative.inc/mastra-ai-the-complete-guide-to-the-typescript-agent-framework-2026) · [Stagehand v3](https://www.browserbase.com/blog/stagehand-v3) · [Browserbase](https://www.browserbase.com/stagehand) · [Cloudflare durable agents](https://developers.cloudflare.com/workflows/get-started/durable-agents/) · [self-hosted AI stack](https://www.tooljunction.io/blog/self-hosted-ai-stack-2026)

Identity/A2A: [A2A spec](https://a2a-protocol.org/v0.2.5/specification/) · [A2A explained](https://onereach.ai/blog/what-is-a2a-agent-to-agent-protocol/) · [A2A security](https://securew2.com/blog/a2a-protocol-security) · [agent auth 2026](https://fluxapay.xyz/learning/how-ai-agents-authenticate-across-platforms-2026) · [AI identity standards (arXiv)](https://arxiv.org/pdf/2604.23280)
