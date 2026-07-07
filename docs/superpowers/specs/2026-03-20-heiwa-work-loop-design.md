# Heiwa Work Loop Design

**Date:** 2026-03-20
**Author:** Devon + Claude Code
**Status:** Approved (pending spec review)

## Summary

Heiwa becomes a single continuous work loop running 24/7 on Railway — the agentic mirror of Devon's intent. All projects absorb into the monorepo. The Captain agent is renamed Heiwa and becomes the orchestrator. Role-based agents (Spine, Executor, Telemetry, Messenger) are replaced with provider-scoped agents (Claude, Gemini, Antigravity, Codex, Qwen) mapped to real rate groups. The trading cockpit (mac-agent) moves to Railway as `apps/heiwa_trading/`. AI-DJ is archived as a shipped product. Strategy evolution follows Karpathy's autoresearch model: modify, evaluate, keep or discard, iterate.

## Architecture

### The Work Loop

Heiwa runs a single continuous async loop on Railway. This replaces the current event-driven Captain that waits for input.

```
while True:
    1. INGEST  — pull from all intent sources
    2. PRIORITIZE — score each item
    3. EXECUTE — pick the top affordable item, delegate to best agent
    4. REPORT — record decision memo, notify if relevant
    5. SLEEP — adaptive delay (5-30s based on queue depth)
```

The loop is not a tight spin. When idle, it sleeps longer. When intent arrives (Discord message, new issue, market data), it processes promptly. The market supervisor tick becomes a cron event feeding the queue — not a separate daemon.

### Intent Sources

Five sources feed one priority queue. Everything becomes a `WorkItem` in SpacetimeDB.

| Source        | Trigger                                       | Default Priority               |
| ------------- | --------------------------------------------- | ------------------------------ |
| Discord       | DM or mention from Devon                      | 0.7 (human spoke)              |
| GitHub Issues | new/updated issue                             | 0.5 (structured work)          |
| goals.md      | file changed since last read                  | 0.6 (Devon updated priorities) |
| Cron          | scheduled interval (market tick, health)      | 0.3 (routine)                  |
| System        | alert/event (deploy fail, rate limit, health) | 0.9 (something broke)          |

### WorkItem Schema

```
WorkItem {
  id:                 uuid
  source:             "discord" | "github" | "goals" | "cron" | "system"
  intent:             string
  intent_class:       IntentEnum       # from existing cognition pipeline
  priority:           float            # 0.0 - 1.0
  status:             "queued" | "active" | "done" | "failed" | "blocked"
  created_at:         timestamp
  started_at:         timestamp?
  result:             string?
  tool_used:          string?
  assigned_agent:     string?          # which provider-scoped agent is executing
  estimated_cost_class: int?           # 1-4, set at dispatch time
  branch_name:        string?          # git branch for autoresearch execution
  parent_id:          uuid?            # if decomposed from another WorkItem
  goal_id:            string?          # reference to goals.md goal (top-level intent)
  issue_ref:          string?          # github issue number
  retry_count:        int              # max 3
}
```

**Relationship to existing Proposal table:** WorkItem replaces the existing `Proposal` table in SpacetimeDB. The `MissionRecord`, `MissionStepRecord`, and `CellRunRecord` tables are also superseded — missions become goals, mission steps become WorkItems, cell runs become DecisionMemos. Migration: existing proposals are converted to WorkItems with `source: "system"` during Phase 2 deployment. Old tables are kept read-only for 30 days, then dropped.

### Priority Scoring

```python
priority = (
    urgency_weight   * urgency      +  # is something broken?
    importance_weight * importance   +  # does goals.md reference this?
    staleness_weight  * staleness    +  # how long waiting?
    cost_weight       * affordability   # do we have rate budget?
)
```

The orchestrator checks affordability before picking an item. If the top item needs Claude Code but Anthropic's rate group is exhausted, it picks the next item that fits an available agent.

## Agent Architecture

### Provider-Scoped Agents

Agents map to real execution resources and rate groups, not abstract roles.

```
Heiwa (orchestrator)
│   THE work loop. Ingests intent, prioritizes, delegates, reports.
│   Runs on whatever model has budget (typically Gemini Flash,
│   falls back through the cascade like any other task).
│
├── Claude (agent)
│   Provider: Anthropic (Claude Pro, $31/mo)
│   Rate:    ~40 turns / 5hr
│   Strength: Deep reasoning, complex code, architecture
│   Surface:  Claude Code CLI subprocess on Railway
│
├── Gemini (agent)
│   Provider: Google (AI Pro, free until Dec 2026)
│   Rate:    ~50 turns / hr
│   Strength: Long context, research, enrichment, fast iteration
│   Surface:  Gemini CLI subprocess on Railway
│
├── Antigravity (agent)
│   Provider: Google (AI Pro, separate rate group)
│   Rate:    ~35 turns / hr
│   Strength: Browser automation, visual tasks, parallel capacity
│   Surface:  Antigravity CLI subprocess on Railway
│
├── Codex (agent)
│   Provider: OpenAI (ChatGPT Plus, $27/mo)
│   Rate:    ~25 turns / hr
│   Strength: Code generation, pragmatic execution
│   Surface:  Codex CLI subprocess on Railway
│
└── Qwen (agent)
    Provider: Ollama (local, unlimited)
    Rate:    unlimited (boost node only)
    Strength: Fast local inference, embeddings, drafts, triage
    Surface:  Ollama API on MacBook boost node (when online)
```

### Rate Cascade

Heiwa itself is in the cascade. The orchestrator logic runs on whatever is cheapest and available. No agent is privileged.

```
1. Gemini CLI      — 50 turns/hr   (free)
2. Antigravity     — 35 turns/hr   (free)
3. Claude Code     — 40 turns/5hr  ($31/mo)
4. Codex           — 25 turns/hr   ($27/mo)
5. Free APIs       — unlimited-ish (free)
6. Qwen/Ollama     — unlimited     (boost node only)
```

All rate groups are usage-checked before any agent dispatches. The dispatch function:

```python
async def pick_agent_and_model(self, work_item: WorkItem) -> Dispatch:
    intent = work_item.intent_class
    budgets = await self.db.get_rate_budgets()
    candidates = self.rank_agents(intent, budgets)
    for agent in candidates:
        if budgets[agent.rate_group].has_capacity():
            return Dispatch(agent=agent, model=agent.primary_model)
    return Dispatch.DEFERRED
```

### What Gets Replaced

| Old Agent                   | Replacement                                     |
| --------------------------- | ----------------------------------------------- |
| Captain (`heiwa_agent.py`)  | Heiwa orchestrator (the work loop)              |
| Spine (`spine.py`)          | Node registry → SpacetimeDB table + health cron |
| Executor (`executor.py`)    | Provider-scoped agents (Claude, Gemini, etc.)   |
| Telemetry (`telemetry.py`)  | Cron function Heiwa calls                       |
| Messenger (`messenger.py`)  | Discord I/O in Heiwa's loop                     |
| OrchestrationService (stub) | Deleted — Heiwa IS the orchestration            |
| DeliveryManager (stub)      | Deleted — dispatch IS the delivery              |

### Autoresearch Model (Karpathy)

Applied to Heiwa's execution cycle. The pattern: modify → evaluate → keep or discard → iterate. Git handles atomic keep/discard.

```
For each WorkItem:
  1. PLAN     — decompose into a concrete change
  2. DELEGATE — pick agent via rate cascade
  3. EXECUTE  — agent works in a git branch
  4. EVALUATE — tests pass + metric improves?
               Yes → git commit, keep
               No  → git revert, discard
  5. RECORD   — DecisionMemo either way
  6. ITERATE  — next WorkItem
```

Evaluation metrics vary by app:

| App             | Metric                                                      |
| --------------- | ----------------------------------------------------------- |
| `heiwa_trading` | Tests pass + paper PnL improves + no risk policy violations |
| `heiwa_hub`     | Tests pass + smoke tests pass + no regressions              |
| `heiwa_web`     | Build succeeds + lighthouse score maintained                |

## Decision Memos

Every action the system takes gets a structured record in SpacetimeDB.

```
DecisionMemo {
  id:            uuid
  work_item_id:  uuid
  agent_id:      string        # which provider-scoped agent (claude, gemini, etc.)
  timestamp:     datetime
  intent:        string
  decision:      string
  reasoning:     string
  tool_used:     string
  result:        string
  duration_ms:   int
  cost_class:    int          # 1-4
}
```

## Goals Interface

### goals.md

Located at `config/goals.md`. Devon's steering wheel. Heiwa reads, decomposes, and works through it.

```markdown
# Goals

## Active

- Evolve trading strategies using autoresearch loop
  scope: adaptive
  horizon: long-term
  confidence: theory
  notes: >
  Initial hypothesis. No experiment data yet.
  - Mutate probability_bias and momentum_weight parameters
  - Evaluate against 24hr simulated PnL
  - Stack improvements that beat baseline

## Paused

## Proposed

## Done
```

### Three Axes

| Axis           | Values                     | Meaning                                   |
| -------------- | -------------------------- | ----------------------------------------- |
| **Scope**      | `definitive` · `adaptive`  | Can agents adjust sub-goals and approach? |
| **Horizon**    | `short-term` · `long-term` | Days vs weeks/ongoing                     |
| **Confidence** | `theory` · `reality`       | Hypothesis vs validated by execution      |

### Agent Permissions on Goals

| Agents CAN                                                    | Agents CANNOT                                 |
| ------------------------------------------------------------- | --------------------------------------------- |
| Add/remove/reword sub-goals under `adaptive` goals            | Change a goal's top-level intent              |
| Update `confidence` from `theory` → `reality` (with evidence) | Delete or Pause a goal                        |
| Append to `notes` with execution findings                     | Change `scope` from `definitive` → `adaptive` |
| Narrow scope based on experiment results                      | Widen scope beyond original intent            |
| Propose new goals (appended to `## Proposed`)                 | Add directly to `## Active`                   |

### Goal Lifecycle

```
Devon writes goal (Active, adaptive, theory)
  → Heiwa decomposes into GitHub Issues → WorkItems
  → Agents execute, learn, update notes + sub-goals
  → Confidence moves theory → reality as evidence stacks
  → Scope narrows as dead ends are pruned
  → Agents propose related goals → Proposed section
  → Devon promotes or dismisses
  → All sub-goals done → goal moves to Done with summary
```

## Monorepo Absorption

### mac-agent → `apps/heiwa_trading/`

```
apps/heiwa_trading/
├── src/heiwa_trading/        # renamed from polymarket_foundation
│   ├── supervisor.py          # cron-triggered function, not daemon
│   ├── cockpit.py             # serves via Hub's FastAPI
│   ├── market_data.py
│   ├── strategy.py
│   ├── formulas.py
│   ├── paper_trader.py
│   ├── tournament.py
│   ├── coinmarketcap.py
│   ├── types.py
│   ├── config.py
│   └── evolution.py           # NEW: autoresearch strategy mutation
├── web/
│   ├── cockpit.html
│   ├── cockpit.css
│   └── cockpit.js
├── runtime/                   # gitignored
├── tests/
├── CONTEXT.md
└── pyproject.toml
```

**Key changes:**

- Supervisor → cron-triggered function (not launchd daemon)
- Cockpit → routes on Hub's FastAPI (`/trading/*`)
- State → SpacetimeDB (JSON files become fallback/cache)
- Branding → "Heiwa Trading" everywhere
- Config → `config/trading/` in monorepo

### ai-dj → `apps/heiwa_dj/`

Archive pointer only. Code stays in `~/ai-dj/` as standalone Electron app.

```
apps/heiwa_dj/
├── README.md       # "Shipped v1.7.0. Standalone at ~/ai-dj/"
└── CONTEXT.md
```

### Home Directory Cleanup

| Action          | Target                                    |
| --------------- | ----------------------------------------- |
| Delete          | `~/mac-agent/`, `~/.mac-agent/`           |
| Delete          | `~/hub.db`, `~/bitcrap/`, `~/R&D/`        |
| Keep            | `~/ai-dj/` (standalone shipped app)       |
| Keep            | `~/heiwa_archive/` (historical reference) |
| Already deleted | `~/heiwa-limited/`                        |

**After cleanup, `~/` contains:** `heiwa`, `ai-dj`, `heiwa_archive`.

## Trading on Railway

### Hub Integration Points

**1. Cockpit routes on FastAPI:**

```
/trading/cockpit     → cockpit.html (static)
/trading/api/state   → supervisor state
/trading/api/action  → operator controls
/trading/sse         → delta-only SSE stream
```

**2. Market supervisor as cron WorkItem:**

```
Every 60s → WorkItem {
  source: "cron",
  intent: "trading:market_tick",
  priority: 0.3
}
```

**3. SpacetimeDB tables:**

- `trading_cohorts` — cohort state
- `trading_wallets` — wallet equity, trades, strategy params
- `trading_markets` — normalized market data
- `trading_market_snapshots` — time-series for backtesting
- `strategy_experiments` — autoresearch results

### Strategy Evolution (Autoresearch)

```python
async def run_evolution_cycle(cohort, market_snapshot) -> StrategyExperiment:
    """
    1. Pick current best variant
    2. Mutate parameters (small perturbation)
    3. Simulate N trades against market snapshot
    4. Compare PnL to baseline
    5. Keep if better, discard if worse
    6. Record as DecisionMemo
    """
```

Pure simulation — no rate limits consumed, no LLM needed for mutations. LLM involvement comes when Heiwa analyzes results and updates goal scope.

## Error Handling

| Failure                     | Response                                                                |
| --------------------------- | ----------------------------------------------------------------------- |
| All rate groups exhausted   | Defer work, sleep until reset, notify Devon via Discord                 |
| Agent subprocess crash      | Record failure, git revert partials, re-queue (max 3 retries)           |
| Railway restart             | Resume from SpacetimeDB. Re-queue any `active` WorkItems                |
| SpacetimeDB unreachable     | In-memory queue, retry connection every 30s, no execution without state |
| Market API down             | Skip tick, retry next cycle, notify after 5 consecutive failures        |
| Git conflict                | Discard experiment branch, record failure, continue                     |
| Boost node offline mid-task | Re-route to Railway agent, WorkItem stays active                        |

## Approval Gates

| Action                         | Requires Devon                              |
| ------------------------------ | ------------------------------------------- |
| Push to `main`                 | Always — Heiwa works in branches, opens PRs |
| Deploy to Railway              | PR merge triggers CI auto-deploy            |
| Spend real money               | Never — paper trading only                  |
| Promote Proposed → Active goal | Always                                      |
| Delete/Pause an Active goal    | Always                                      |
| Widen goal scope               | Always                                      |

## Observability

### Three Surfaces

1. **Discord** (real-time) — decisions needing attention, daily summaries, errors
2. **Cockpit UI** (visual) — `/trading/cockpit` for trading, `/hub/dashboard` for system-wide
3. **SpacetimeDB** (queryable) — work_items, decision_memos, rate_budgets, strategy_experiments

### Daily Digest (Discord)

```
Heiwa Daily — March 22, 2026

Work: 47 items processed, 3 failed, 2 deferred
Rate usage: Gemini 82%, Antigravity 41%, Claude 35%, Codex 12%
Trading: 12 evolution experiments, 3 kept (PnL +$1.22 stacked)
Goals: "Add Kalshi" 2/4 issues done, "Evolve strategies" ongoing
Errors: CoinMarketCap API timeout (3x, non-critical)
Proposed: 1 new goal suggested — review in goals.md
```

## Existing Code Integration

### Preserve & Extend

| Component                               | Action                                                                     |
| --------------------------------------- | -------------------------------------------------------------------------- |
| `rate_ledger.py`                        | Extend with STDB persistence and API exposure — already has correct limits |
| Cognition pipeline (intent/risk/router) | Rewire from Spine into Heiwa's work loop                                   |
| `cognition/approval.py`                 | Preserve — Discord approval workflow stays                                 |
| `SecurityService`                       | Preserve — centralized auth + secret redaction unchanged                   |
| `HeiwaClaw` + `ToolMesh`                | Preserve — execution gateway unchanged                                     |
| `heiwa_skills/` (30+ YAML templates)    | Wire into agent dispatch — agents can execute skills                       |
| `mcp_server.py` (FastAPI app)           | Extend with trading routes and hub dashboard                               |

### Delete

| Component                        | Reason                             |
| -------------------------------- | ---------------------------------- |
| `OrchestrationService` (stub)    | Heiwa's loop replaces it           |
| `DeliveryManager` (stub)         | Dispatch replaces it               |
| Role-based agent files (Phase 4) | Replaced by provider-scoped agents |

### Rewrite

| Component                            | Reason                                                 |
| ------------------------------------ | ------------------------------------------------------ |
| `ai_router.json`                     | Reflect agent roster, not node-based model assignments |
| `profiles.json` (HeiwaCells)         | Personas agents adopt, not routing targets             |
| `HEIWA.md`, `AGENTS.md`, `CLAUDE.md` | Reflect new architecture                               |
| `END_STATE_2026-03.md`               | Update target to match this design                     |
| `deploy.yml`                         | Add smoke tests for work loop, trading, autoresearch   |

## Implementation Phases

### Phase 1: Absorb & Rebrand

- Move mac-agent → `apps/heiwa_trading/`, rename to `heiwa_trading`
- Rebrand cockpit UI → "Heiwa Trading"
- Add CONTEXT.md
- Mount trading routes on Hub's FastAPI (`/trading/*`)
- Add `apps/heiwa_dj/` archive pointer
- Clean up home directory
- Commit, push, verify Railway deploys
- Update HEIWA.md app directory table

### Phase 2: State & Data Foundation

- New SpacetimeDB tables: work_items, decision_memos, trading_cohorts, trading_wallets, trading_markets, trading_market_snapshots, strategy_experiments
- Migrate trading state from JSON → SpacetimeDB
- Extend `rate_ledger.py` with STDB persistence and API exposure
- Implement DecisionMemo write path
- Cockpit SSE → delta-only from STDB
- Delete OrchestrationService and DeliveryManager stubs
- Tests for all new tables and state operations
- Update rooms/sdk.md

### Phase 3: The Work Loop

- Implement goals.md schema (scope/horizon/confidence, agent-writable)
- Implement WorkItem priority scoring
- Build intent ingestion (5 sources)
- Rewrite Captain → Heiwa orchestrator with continuous loop
- Rewire cognition pipeline from Spine into Heiwa's loop
- Preserve approval workflow (cognition/approval.py + Discord)
- Rate cascade dispatch using existing rate_ledger.py
- Convert market supervisor to cron WorkItem
- Goal decomposition: goals.md → GitHub Issues → WorkItems
- Goal mutation: agents update adaptive goals
- Discord two-way
- Tests + CI smoke tests for work loop
- Update HEIWA.md, AGENTS.md, CLAUDE.md

### Phase 4: Provider-Scoped Agents

- Replace role-based agents with Claude, Gemini, Antigravity, Codex, Qwen
- Each: subprocess wrapper + rate tracking + timeout + git branch isolation
- Rewrite ai_router.json for agent roster
- Refactor profiles.json — personas, not routing targets
- Wire heiwa_skills/ into agent dispatch
- Telemetry → cron function
- Discord I/O → Heiwa loop
- Node registry → STDB table + health cron
- Qwen activates via /ws/worker
- Delete old agent files
- Update all architecture docs and CI

### Phase 5: Autoresearch for Trading

- evolution.py: mutation, simulation, evaluation
- Keep/discard via git branch pattern
- Result analysis: Heiwa reads experiments, narrows scope, updates confidence
- Hub dashboard: experiment history, stacked improvement graph
- Daily digest to Discord
- CI: autoresearch cycle smoke test

### Phase 6: Observability & Polish

- Hub dashboard at /hub/dashboard
- Goal progress, rate budget timelines, decision memo browser
- PR workflow for main-branch changes
- Daily/weekly digest refinement
- Final doc pass: all md files and CONTEXT.md files

## Design Decisions & Clarifications

### SpacetimeDB Module Strategy

New tables (work_items, decision_memos, rate_budgets, trading_*) are added to the production STDB module at `apps/heiwa_hub/spacetimedb/src/lib.rs`, which already has 20+ tables (Proposals, Nodes, Runs, etc.). The scaffold module at `heiwaproductiondb/` is deleted.

WorkItem replaces the existing Proposal table. MissionRecord/MissionStepRecord/CellRunRecord are superseded (missions → goals, steps → WorkItems, cell runs → DecisionMemos). During Phase 2 deployment, existing proposals are migrated to WorkItems. Old tables are kept read-only for 30 days, then dropped.

### Orchestrator LLM Budget

The orchestrator's core loop (ingest, score priorities, dispatch) is **entirely deterministic** — no LLM calls. Priority scoring is formulaic. Rate budget checks are arithmetic. Cron scheduling is clock-based.

LLM calls are only needed for:

- **Goal decomposition** (reading goals.md, creating GitHub Issues) — happens on goals.md change, not every tick
- **Experiment analysis** (interpreting autoresearch results) — happens after batches of experiments, not per-experiment
- **Discord interpretation** (understanding natural language intent from Devon) — happens on message receipt

These are **WorkItems themselves**, dispatched through the same cascade. Goal decomposition is a WorkItem with `intent_class: ORCHESTRATE`. The orchestrator does not consume a separate LLM budget — it creates work that goes through the queue like everything else.

When all rate groups are exhausted, the deterministic loop continues running: it ingests, scores, and discovers it cannot dispatch anything. It defers, sleeps, and notifies Devon. No LLM needed for this path.

### Concurrency Model

The work loop is **single-threaded and sequential** in Phase 3. One WorkItem executes at a time. The loop blocks while an agent subprocess runs.

**Agent subprocess constraints:**

- Maximum 1 concurrent subprocess (Phase 3-4 baseline)
- Timeout: 15 minutes for standard tasks, 30 minutes for complex code tasks (configurable per intent_class)
- Memory: Railway container limit (512MB base, scales to 2GB under Pro)
- If a subprocess exceeds timeout, it is killed, the WorkItem is marked failed, partial changes are reverted

**Parallel execution (future, not in this spec):** Phase 4 could be extended to run 2-3 non-overlapping agents concurrently (e.g., Claude on a heiwa_hub task while Gemini researches for heiwa_trading). This requires file-level lock tracking to prevent overlapping edits. Deferred — single-threaded is correct until proven insufficient.

### Goals.md Write Coordination

Only the Heiwa orchestrator writes to goals.md. Agents do not write directly.

When an agent completes work that should update a goal (narrowing scope, updating confidence, adding notes), it returns structured metadata in its result:

```python
AgentResult {
  work_item_id: uuid
  output: string
  goal_updates: GoalUpdate? {    # optional
    goal_id: string
    update_type: "narrow_scope" | "update_confidence" | "append_notes"
    content: string
    evidence: list[str]          # DecisionMemo IDs
  }
}
```

The orchestrator receives this result, validates the update against the agent permission rules, and writes to goals.md in a single atomic commit. No race condition — one writer, serialized through the loop.

### Transport Layer for Subprocess Agents

The existing `LocalBusTransport` (in-process pub/sub) is removed in Phase 4. Provider-scoped agents are subprocesses — they cannot share an in-process bus.

**Replacement:** Direct subprocess invocation and result capture. The orchestrator:

1. Creates a git branch for the task
2. Writes a task brief to a temp file (the "program.md" equivalent)
3. Spawns the CLI subprocess (e.g., `claude --task-file /tmp/task-brief.md`)
4. Reads stdout/stderr for progress and result
5. Parses the structured result (exit code + output file)
6. Evaluates (tests pass? metric improved?)
7. Keeps or discards the branch

No IPC protocol needed. The interface is: filesystem (task brief in, code changes out) + stdout (progress) + exit code (success/failure). This is the same model Karpathy's autoresearch uses — the agent gets a file telling it what to do and modifies code in response.

### Qwen/Boost Node Availability

Qwen is position 6 in the cascade and is **not an always-available fallback**. When the MacBook boost node is offline, Qwen is unavailable. The effective cascade becomes positions 1-5 only.

When all cloud rate groups (positions 1-5) are exhausted and the boost node is offline, work is deferred. This is the same behavior as the "All rate groups exhausted" error handler. The system does not break — it sleeps and waits for rate windows to reset.

The cascade diagram is accurate as written. Qwen is listed with "(boost node only)" to make the availability constraint visible.

### Skill Execution Integration

Provider-scoped agents receive skill templates as part of their task brief. The orchestrator:

1. Matches the WorkItem's intent_class to available skills in `heiwa_skills/`
2. If a matching skill exists, includes the YAML template in the task brief
3. The agent follows the skill template (tool calls, MCP invocations, validation steps)
4. If no skill matches, the agent works from the raw intent description

Skills are guidance, not enforcement. An agent can deviate from a skill template if the situation requires it. The DecisionMemo records whether a skill was used and whether the agent followed or deviated from it.

### ACP (Agent Communication Protocol)

ACP as described in END_STATE is superseded by this design. Provider-scoped agents do not communicate with each other — they communicate with the orchestrator through the subprocess result interface. The orchestrator is the sole coordinator.

If agent-to-agent delegation is ever needed (e.g., Claude discovers mid-task that it needs Gemini to research something), it returns a result requesting a follow-up WorkItem. The orchestrator creates the new WorkItem and dispatches it through the normal cascade. This is sequential delegation, not peer-to-peer ACP.

END_STATE_2026-03.md should be updated to reflect this change.

### Phase Dependency Chain (Agent Transition)

Phase 3 extracts business logic from Spine and Executor into the Heiwa orchestrator:

- Spine's request routing → orchestrator's dispatch function
- Spine's node registry → SpacetimeDB table + health cron function
- Executor's HeiwaClaw invocation → extracted into a shared utility the orchestrator and future agents both call

Phase 4 then:

- Creates new provider-scoped agent subprocess wrappers that use the extracted HeiwaClaw utility
- Deletes the now-empty Spine, Executor, Telemetry, Messenger shells

Between Phases 3 and 4, the system runs with: Heiwa orchestrator (new) + Executor (legacy, for HeiwaClaw calls) + Telemetry (legacy, as cron) + Messenger (legacy, for Discord). Phase 4 completes the transition.
