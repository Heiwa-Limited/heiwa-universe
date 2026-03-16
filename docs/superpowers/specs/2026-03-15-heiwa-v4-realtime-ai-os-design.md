# Heiwa v4: Realtime AI Operating System

**Date**: 2026-03-15
**Author**: Devon (dmcgregsauce) + Claude Code (Opus 4.6)
**Status**: Draft — awaiting review

---

## 1. Vision

Heiwa is a self-hosted, self-improving AI operating system. It routes work across multiple LLM providers with cost awareness, dispatches specialist agents at calibrated effort levels, accumulates knowledge from every execution, and improves autonomously — all on a single state substrate (SpacetimeDB) that keeps every component in realtime sync.

**Core principle**: Heiwa knows who/what/why/where/when/how about itself at all times. No agent waits on stale data. No state lives outside STDB.

---

## 2. What Exists Today (v3)

Working:
- Cognition pipeline: IntentNormalizer (regex-first, 0.95 confidence) → RiskScorer (deterministic keyword escalation) → ComputeRouter (4 compute classes, rate-group-aware fallback)
- Hub agents: Spine (fleet orchestration), Executor (task execution), Captain (health monitor), Telemetry, Messenger (Discord)
- HeiwaClaw gateway: resolves BrokerRouteResult → dispatch via ToolMesh subprocess
- CLI: `heiwa task`, `heiwa status`, `heiwa cells`, `heiwa bench`
- Local bus transport (NATS eliminated)
- SQLite/Postgres/STDB multi-backend abstraction

Problems:
- heiwaclaw.py is 95KB — monolith hiding in a module
- Agents are stateless — no memory across executions
- Embedding model (qwen3-embedding:0.6b) pulled but unused
- Multi-backend db.py adds complexity for no benefit
- No per-model effort/reasoning configuration
- No feedback loops — system doesn't learn from outcomes
- Captain monitors but doesn't act autonomously

---

## 3. Architecture: Heiwa v4

### 3.1 Single State Substrate: SpacetimeDB

Everything that can change at runtime lives in STDB. No SQLite. No Postgres. No JSON config files for runtime state. Git-checked seed files bootstrap tables on first deploy; STDB is the runtime truth.

**Why**: Agents subscribe to table changes via WebSocket. When the Captain updates a model's effort level based on feedback, the ComputeRouter sees it instantly. No polling, no cache invalidation, no stale reads.

#### STDB Tables

**Model Tier Matrix**
```
table model_tiers {
    model_id: String,           // internal alias: "ollama/qwen3.5:4b"
    provider_model_id: String,  // actual API string: "qwen3.5:4b" (what gets sent to the provider)
    provider: String,           // "ollama"
    rate_group: String,         // "local_ollama"
    capability_class: u8,       // 1=light, 2=medium, 3=heavy
    effort_knob: String,        // provider-specific: "thinking:off", "effort:medium", "reasoning:xhigh"
    effort_level: u8,           // normalized 1-5 scale
    cost_per_turn: f64,         // 0.0 for local/free, estimated for subscription
    max_context_tokens: u32,    // model's context window
    strengths: Vec<String>,     // ["code_generation", "research", "audit"]
    enabled: bool,
    last_success_rate: f64,     // rolling 20-execution window
    avg_latency_ms: u64,       // rolling average
    latency_p95_ms: u64,       // 95th percentile — catches outlier spikes avg hides
    updated_at: Timestamp,
}
```

**Task Dispatch Envelope**
```
table task_dispatches {
    task_id: String,
    parent_task_id: String,     // null for top-level; set when Captain directive spawns sub-tasks
    intent_class: String,       // "audit", "build", "research", etc.
    risk_level: String,         // "low", "medium", "high", "critical"
    assigned_model: String,     // FK to model_tiers.model_id
    effort_knob: String,        // resolved effort for this dispatch
    assigned_cell: String,      // specialist agent personality
    budget_max_turns: u8,       // turn cap
    budget_max_seconds: u32,    // wall-time cap
    fallback_model: String,     // downgrade target if budget exceeded
    sandbox_mode: String,       // "trusted" | "e2b"
    tools_allowed: Vec<String>, // MCP servers / CLI tools available
    context_files: Vec<String>, // relevant file paths
    status: String,             // "queued" | "running" | "complete" | "failed" | "budget_exceeded"
    result_summary: String,
    tokens_used: u32,
    latency_ms: u64,
    created_at: Timestamp,
    completed_at: Timestamp,
}
```

**Memory: Execution History + Feedback**
```
table execution_memory {
    execution_id: String,
    task_id: String,            // FK to task_dispatches
    model_used: String,
    cell_used: String,
    intent_class: String,
    input_hash: String,         // deduplication
    outcome: String,            // "success" | "partial" | "failed" | "reverted"
    quality_score: f64,         // 0.0-1.0, from feedback or automated checks
    feedback_source: String,    // "automated_test" | "captain_review" | "human"
    learnings: String,          // free-text: what worked, what didn't
    created_at: Timestamp,
}
```

**Memory: Knowledge Embeddings**
```
table knowledge_embeddings {
    embedding_id: String,
    source_type: String,        // "code_file" | "commit" | "execution_result" | "document" | "conversation"
    source_ref: String,         // file path, commit SHA, task_id, etc.
    chunk_text: String,         // the actual text chunk
    embedding: Vec<f64>,        // vector from qwen3-embedding:0.6b
    metadata: String,           // JSON: tags, timestamps, relevance scores
    created_at: Timestamp,
}
```

**Agent Registry**
```
table agent_registry {
    agent_id: String,
    agent_type: String,         // "captain" | "executor" | "cell"
    display_name: String,
    status: String,             // "online" | "offline" | "busy" | "error"
    current_task_id: String,
    model_preference: String,   // preferred model_id
    capabilities: Vec<String>,  // ["code_review", "research", "deploy"]
    node_id: String,            // which node it's running on
    last_heartbeat: Timestamp,
    lifetime_tasks: u32,
    lifetime_success_rate: f64,
}
```

**Node Registry**
```
table node_registry {
    node_id: String,            // "macbook@heiwa-node-a", "railway@heiwa-cloud-hq"
    node_type: String,          // "boost" | "cloud" | "edge"
    status: String,             // "online" | "offline"
    capabilities: Vec<String>,  // ["ollama", "gpu_m4", "filesystem", "docker"]
    available_models: Vec<String>,
    cpu_percent: f64,
    memory_percent: f64,
    last_heartbeat: Timestamp,
}
```

**Rate Group State**
```
table rate_group_state {
    rate_group: String,         // "claude_code", "google_gemini_cli", etc.
    turns_used: u32,
    turns_max: u32,
    window_start: Timestamp,
    window_seconds: u32,
    cooldown_until: Timestamp,
    available: bool,
}
```

**Captain Directives**
```
table captain_directives {
    directive_id: String,
    directive_type: String,     // "self_check" | "model_tune" | "cell_deploy" | "repo_audit"
    schedule_cron: String,      // "*/30 * * * *" (every 30 min)
    last_run: Timestamp,
    next_run: Timestamp,
    enabled: bool,
    config: String,             // JSON: parameters for this directive type
}
```

### 3.2 Model Tier Matrix

Every model from every provider gets a row in `model_tiers` with its effort knob mapped to a normalized 1-5 scale.

#### Provider Effort Mapping

| Provider | Control Name | Values | Normalized |
|----------|-------------|--------|------------|
| Claude (Opus/Sonnet) | `effort` | low, medium, high, auto | 1, 3, 5, auto |
| Codex/OpenAI (gpt-4.1, gpt-5.4) | `reasoning_effort` | low, medium, high, xhigh | 1, 3, 4, 5 |
| Ollama (Qwen 3.5 MoE) | `/think` toggle | off, on | 1, 4 |
| Gemini CLI (Gemini 3 Flash) | thinking toggle | off, on | 1, 4 |
| Gemini CLI (Gemini 3.1 Pro) | thinking level | low, high | 2, 5 |
| Antigravity (all models) | always thinking | always on | 4 (fixed) |

#### Seed Configuration (loaded on first boot)

```
ollama/qwen3.5:4b          → light-medium tasks, thinking:off for audit, thinking:on for code
ollama/qwen2.5-coder:1.5b  → light tasks only, code-specific, no thinking
ollama/qwen2.5-coder:0.5b  → ultra-light: linting, formatting, simple lookups
ollama/llama3.2:3b          → light chat/general, no code generation
gemini-cli/gemini-3-flash   → medium tasks, free tier, burn freely, thinking:on for research
gemini-cli/gemini-3.1-pro   → heavy tasks, free tier, thinking:high for architecture
antigravity/gemini-3-auto   → medium-heavy, separate rate group, always thinking
codex/gpt-4.1               → medium code tasks, reasoning:medium default
codex/gpt-5.4               → heavy only, reasoning:xhigh, reserved for architecture/review
claude/sonnet-4-6           → medium-heavy, effort:medium for code, effort:high for review
claude/opus-4-6             → heavy only, effort:high, reserved for adversarial review
```

#### Auto-Tuning

The Captain reads `execution_memory` feedback and updates `model_tiers.last_success_rate` and `model_tiers.avg_latency_ms`. If a model at effort level N fails >30% of tasks for an intent class, Captain bumps that intent's minimum effort level in the routing rules. All subscribers (ComputeRouter) see the change instantly via STDB subscription.

### 3.3 Memory Layer

Three components, all backed by STDB:

**Execution Memory** — every task dispatch records its outcome, quality score, and learnings. The Captain reads this to detect patterns: "research tasks on Gemini Flash at thinking:off fail 60% of the time" → auto-tune.

**Knowledge Embeddings** — the embedding pipeline runs on `qwen3-embedding:0.6b` (local, unlimited). Sources:
- Code files in the repo (re-indexed on git push)
- Execution results (what the agent produced)
- Commit messages and PR descriptions
- Documents in `docs/`
- Conversation excerpts (from Discord/CLI interactions)

Retrieval: when an agent is dispatched, the context_files in the dispatch envelope are enriched with semantically similar chunks from `knowledge_embeddings`. The agent gets relevant context without loading the entire repo.

**Pruning strategy**: Embeddings carry a `created_at` timestamp and an implicit relevance score (how often they're retrieved). A Captain directive runs weekly to prune: code file embeddings are re-indexed on every git push (stale chunks deleted); execution result embeddings older than 30 days with zero retrievals are pruned; document embeddings are re-indexed when source files change (hash comparison).

**Feedback Collector** — three input channels:
1. **Automated**: test pass/fail after code changes, lint scores, PR merge/reject
2. **Captain review**: Captain spot-checks execution results using a cheap model
3. **Human**: operator thumbs-up/down via CLI or Discord reaction

### 3.4 Captain: Always-On Orchestrator

The Captain is not a monitor — it's the brain. It runs on Railway (always-on) and orchestrates via STDB subscriptions.

**Core loop** (subscription-driven, not polling):
1. Subscribe to `captain_directives` — when `next_run` passes, execute the directive
2. Subscribe to `task_dispatches` where status = "failed" or "budget_exceeded" — auto-retry with escalated model/effort
3. Subscribe to `rate_group_state` — when a rate group recovers from cooldown, drain any queued tasks
4. Subscribe to `execution_memory` — when quality_score patterns degrade, update `model_tiers`

**Captain trust boundaries**:
- **Autonomous (no approval)**: read-only audits, test runs, status checks, model tier tuning, embedding re-indexing
- **Auto-fix with guard rails**: code changes on non-main branches only, PR creation (never direct push to main), max 50 lines changed per fix
- **Requires human approval**: dependency changes, config changes affecting production, any risk_level >= "high" task, Dockerfile modifications, STDB schema migrations

**Self-check directive** (runs every 30 minutes):
1. `git diff HEAD~1` — what changed since last check?
2. `pytest --tb=short` — do tests pass?
3. Scan for TODOs, FIXMEs, files >50KB (like heiwaclaw.py)
4. Check rate group health
5. If issues found → create task_dispatch → route through cognition pipeline → execute fix → open PR

**Model tuning directive** (runs hourly):
1. Query `execution_memory` for last 50 executions
2. Group by model + intent_class + effort_level
3. Calculate success rates, avg latency
4. Update `model_tiers` for underperforming combinations
5. Log changes to `execution_memory` as learnings

### 3.5 Agent Dispatch Envelope

When the ComputeRouter selects a model, it constructs a full dispatch envelope:

```python
dispatch = TaskDispatch(
    task_id="cli-task-abc123",
    intent_class="build",
    risk_level="medium",
    assigned_model="codex/gpt-4.1",
    effort_knob="reasoning:medium",
    assigned_cell="senior-backend-engineer",  # from Agency catalog
    budget_max_turns=8,
    budget_max_seconds=300,
    fallback_model="gemini-cli/gemini-3-flash",
    sandbox_mode="trusted",
    tools_allowed=["railway-mcp", "docker-mcp", "gh"],
    context_files=["apps/heiwa_hub/main.py", "packages/heiwa_sdk/heiwaclaw.py"],
)
```

The Executor reads this from STDB (subscription), invokes the model with the correct effort knob, enforces the budget, and writes the result back. Everything observable in realtime.

### 3.6 HeiwaClaw Decomposition

The 95KB heiwaclaw.py splits into:

| Module | Responsibility | Size Target |
|--------|---------------|-------------|
| `heiwaclaw/resolve.py` | BrokerRouteResult → model + effort selection | <500 lines |
| `heiwaclaw/dispatch.py` | Construct TaskDispatch envelope, write to STDB | <300 lines |
| `heiwaclaw/execute.py` | Read dispatch from STDB, invoke model, enforce budget | <500 lines |
| `heiwaclaw/adapters/` | Per-provider adapters (ollama, gemini, codex, claude, openclaw) | <200 lines each |
| `heiwaclaw/acp.py` | ACP protocol adapter for OpenClaw interop | <300 lines |
| `heiwaclaw/mcp.py` | Heiwa's own MCP server (expose capabilities to other tools) | <400 lines |

### 3.7 Protocol Integration

**MCP (Model Context Protocol)**: Heiwa exposes its own MCP server so any Class 3 executor (Claude Code, Gemini CLI, Codex) can call Heiwa directly:
- `heiwa.submit_task(raw_text, identity, surface)` → returns task_id
- `heiwa.get_status(task_id)` → returns dispatch state
- `heiwa.approve(task_id)` → approve pending task
- `heiwa.list_models()` → current tier matrix
- `heiwa.query_memory(query)` → semantic search over knowledge

**ACP (Agent Control Protocol)**: HeiwaClaw speaks ACP for OpenClaw interop. When OpenClaw Gateway is available (locally or on Railway), agents dispatch through ACP. When it's not, they fall back to direct subprocess invocation. OpenClaw is an integration surface, not a dependency.

### 3.8 Cell Catalog (HeiwaCells)

Specialist agent personalities from The Agency (120+ agents) are imported as HeiwaCells. Each cell maps to:
- Intent classes it handles (build → senior-backend-engineer, research → research-scout)
- Minimum capability_class required
- Default model preference
- Personality/system prompt

Cells are stored in STDB (`agent_registry` table) so the Captain can deploy, update, and retire cells at runtime. New cells can be installed from ClawHub.

### 3.9 Portability

**One codebase, two targets:**
- **Local (Mac/Linux)**: `heiwa start` boots the hub, connects to local STDB (`spacetime start local`), discovers Ollama models, registers as boost node
- **Railway (cloud)**: Dockerfile boots the hub, connects to maincloud STDB, CLI tools installed in Docker, Captain runs always-on

**No environment-specific code paths.** The only difference is the STDB connection string and which models are available (Ollama only exists on boost nodes). The `node_registry` table tracks what's where.

**Deploy flow**: push to GitHub → Railway auto-deploys → hub boots → connects to STDB → subscribes to all tables → Captain resumes directives → system is live.

---

## 4. What Gets Deleted

| Current | Replacement |
|---------|------------|
| `db.py` multi-backend abstraction | Direct STDB client only |
| `ai_router.json` (static config) | `model_tiers` STDB table (seed file for bootstrap) |
| SQLite/Postgres deps in requirements.txt | Remove `psycopg2-binary`, SQLite stdlib |
| `tool_mesh.py` subprocess map | `heiwaclaw/adapters/` per-provider modules |
| 95KB `heiwaclaw.py` monolith | `heiwaclaw/` package (6 focused modules) |
| Static HeiwaCells markdown | STDB `agent_registry` + ClawHub sync |
| Polling-based Captain health checks | STDB subscription-driven directives |

---

## 5. What Gets Added

| Component | Purpose |
|-----------|---------|
| STDB Rust module tables (8 tables) | Single state substrate for everything |
| Embedding pipeline | Index code, docs, execution results into `knowledge_embeddings` |
| Feedback collector | Automated + Captain + human quality signals |
| Model auto-tuner | Captain directive that adjusts effort levels based on outcomes |
| Self-check directive | Captain cron: audit repo, run tests, dispatch fixes, open PRs |
| Heiwa MCP server | Expose Heiwa capabilities to external agents |
| ACP adapter | OpenClaw interop for agent dispatch |
| Seed file loader | Bootstrap STDB tables from checked-in config on first boot |
| `heiwa start` command | One-command local boot (STDB + hub + Ollama discovery) |

---

## 6. Implementation Phases

**Phase 1: STDB Foundation + Model Tier Matrix** (~2 days)
- Write STDB Rust module with all 8 tables
- Seed file loader for model_tiers
- Update ComputeRouter to read from STDB subscription instead of ai_router.json
- Delete db.py multi-backend code
- `heiwa start` boots STDB + hub locally
- `heiwa inspect [table]` CLI command — dump any STDB table as formatted output for debugging during development

**Phase 2: Memory Layer + Feedback** (~2 days)
- Embedding pipeline using qwen3-embedding:0.6b
- Index repo code files and docs on boot
- execution_memory writes on every task completion
- Feedback collector (automated test results first)
- Context enrichment in dispatch envelope

**Phase 3: Captain Autonomy** (~2 days)
- Self-check directive (repo audit, test runner, PR creator)
- Model auto-tuner directive (reads execution_memory, updates model_tiers)
- Rate group recovery queue drain
- Failed task auto-retry with escalation

**Phase 4: HeiwaClaw Decomposition + Protocols** (~2 days)
- Split heiwaclaw.py into package
- Per-provider adapters with effort knob support
- Heiwa MCP server
- ACP adapter for OpenClaw

**Phase 5: Cell Catalog + Agency Import** (~1 day)
- Import Agency agent definitions as HeiwaCells
- Cell-to-intent mapping in STDB
- CellSelector in dispatch pipeline
- ClawHub sync for skill distribution

**Phase 6: Railway Deploy** (~1 day)
- Update Dockerfile for STDB client + CLI tools
- Railway 3-service setup (hub + STDB + scheduler)
- Private networking configuration
- Verify Captain runs always-on

---

## 7. Success Criteria

The system is working when:
1. `heiwa start` boots everything locally in under 10 seconds
2. A task submitted via CLI flows through intent → risk → routing → model selection (with correct effort knob) → cell assignment → execution → result stored in STDB — all observable in realtime
3. Captain runs a self-check, finds an issue, dispatches a fix to the cheapest capable model, and opens a PR — without human intervention
4. Captain auto-tunes a model's effort level based on accumulated feedback, and the change propagates to the ComputeRouter within 1 second via STDB subscription
5. `git push` to GitHub triggers Railway deployment that connects to the same STDB and resumes Captain directives seamlessly
6. Any Class 3 executor (Claude Code, Gemini CLI, Codex) can call Heiwa via MCP to submit tasks and query memory

---

## 8. Hard Rules (Inherited + New)

- State: STDB is the only runtime state layer. Period.
- Transport: WebSocket subscriptions. No polling. No REST-only multi-turn sessions.
- Cost: subscription CLI tools + free APIs only. No paid API credits.
- Privacy: sovereign work stays on boost nodes (never cloud).
- Untrusted code: E2B sandboxes only.
- Effort: always use the cheapest model at the lowest effort level that meets the task's requirements.
- Budget: every dispatch carries a turn cap and wall-time cap.
- Memory: every execution writes to execution_memory. No silent failures.
- Feedback: quality signals flow back to model_tiers. The system learns.
- Portability: one codebase, two targets. No environment-specific code.
- Honesty: do not overstate maturity in docs, logs, or agent responses.
