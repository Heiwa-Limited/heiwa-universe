# Phase 1: STDB Foundation + Model Tier Matrix — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the multi-backend database abstraction and static JSON config with SpacetimeDB as the single runtime state layer, starting with the Model Tier Matrix.

**Architecture:** Add 8 new STDB tables to the existing Rust module (19 tables already). Three primary tables (`model_tiers`, `task_dispatches`, `rate_group_state`) get full implementation with Python bridge, seed data, and CLI integration. Five additional tables (`execution_memory`, `knowledge_embeddings`, `agent_registry`, `node_registry`, `captain_directives`) get Rust table definitions + basic reducers only — no Python bridge, seed, or CLI integration in Phase 1. Seed from `ai_router.json` on boot. ComputeRouter subscribes to `model_tiers` instead of reading JSON. New CLI commands `heiwa start` and `heiwa inspect` for local development.

**Tech Stack:** Rust (SpacetimeDB module), Python 3.14, SpacetimeDB CLI 2.0.2, pytest

**Spec:** `docs/superpowers/specs/2026-03-15-heiwa-v4-realtime-ai-os-design.md`

---

## Chunk 1: STDB Rust Module — New Tables + Reducers

### Task 1: Add `model_tiers` table to STDB Rust module

**Files:**

- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`

- [ ] **Step 1: Add the ModelTier struct and table**

Append after the existing table definitions (after line ~1500 in lib.rs):

```rust
#[table(accessor = model_tiers, public)]
pub struct ModelTier {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    #[unique]
    pub model_id: String,           // "ollama/qwen3.5:4b"
    pub provider_model_id: String,  // "qwen3.5:4b" (actual API string)
    pub provider: String,           // "ollama"
    pub rate_group: String,         // "local_ollama"
    pub capability_class: u8,       // 1=light, 2=medium, 3=heavy
    pub effort_knob: String,        // "thinking:off", "effort:medium", "reasoning:xhigh"
    pub effort_level: u8,           // normalized 1-5
    pub cost_per_turn: f64,         // 0.0 for local/free
    pub max_context_tokens: u32,
    pub strengths_json: String,     // JSON array: ["code_generation", "research"]
    pub enabled: bool,
    pub last_success_rate: f64,     // rolling 20-execution window
    pub avg_latency_ms: u64,
    pub latency_p95_ms: u64,
    pub updated_at: String,         // ISO 8601
}
```

- [ ] **Step 2: Add the upsert_model_tier reducer**

```rust
#[reducer]
pub fn upsert_model_tier(
    ctx: &ReducerContext,
    model_id: String,
    provider_model_id: String,
    provider: String,
    rate_group: String,
    capability_class: u8,
    effort_knob: String,
    effort_level: u8,
    cost_per_turn: f64,
    max_context_tokens: u32,
    strengths_json: String,
    enabled: bool,
) -> Result<(), String> {
    let now = ctx.timestamp.to_string();
    if let Some(mut existing) = ctx.db.model_tiers().model_id().find(&model_id) {
        existing.provider_model_id = provider_model_id;
        existing.provider = provider;
        existing.rate_group = rate_group;
        existing.capability_class = capability_class;
        existing.effort_knob = effort_knob;
        existing.effort_level = effort_level;
        existing.cost_per_turn = cost_per_turn;
        existing.max_context_tokens = max_context_tokens;
        existing.strengths_json = strengths_json;
        existing.enabled = enabled;
        existing.updated_at = now;
        ctx.db.model_tiers().id().update(existing);
    } else {
        ctx.db.model_tiers().insert(ModelTier {
            id: 0, // auto_inc
            model_id,
            provider_model_id,
            provider,
            rate_group,
            capability_class,
            effort_knob,
            effort_level,
            cost_per_turn,
            max_context_tokens,
            strengths_json,
            enabled,
            last_success_rate: 1.0,
            avg_latency_ms: 0,
            latency_p95_ms: 0,
            updated_at: now,
        });
    }
    Ok(())
}
```

- [ ] **Step 3: Add the update_model_tier_stats reducer (for Captain auto-tuning)**

```rust
#[reducer]
pub fn update_model_tier_stats(
    ctx: &ReducerContext,
    model_id: String,
    success_rate: f64,
    avg_latency_ms: u64,
    latency_p95_ms: u64,
) -> Result<(), String> {
    if let Some(mut tier) = ctx.db.model_tiers().model_id().find(&model_id) {
        tier.last_success_rate = success_rate;
        tier.avg_latency_ms = avg_latency_ms;
        tier.latency_p95_ms = latency_p95_ms;
        tier.updated_at = ctx.timestamp.to_string();
        ctx.db.model_tiers().id().update(tier);
    }
    Ok(())
}
```

- [ ] **Step 4: Compile the Rust module**

Run:

```bash
cd /Users/dmcgregsauce/heiwa/apps/heiwa_hub/spacetimedb && cargo build
```

Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs
git commit -m "feat(stdb): add model_tiers table with upsert and stats reducers"
```

### Task 2: Add `task_dispatches` table

**Files:**

- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`

- [ ] **Step 1: Add TaskDispatch struct**

```rust
#[table(accessor = task_dispatches, public)]
pub struct TaskDispatch {
    #[primary_key]
    pub task_id: String,
    pub parent_task_id: Option<String>, // None for top-level
    pub intent_class: String,
    pub risk_level: String,
    pub assigned_model: String,     // FK to model_tiers.model_id
    pub effort_knob: String,
    pub assigned_cell: String,
    pub budget_max_turns: u8,
    pub budget_max_seconds: u32,
    pub fallback_model: String,
    pub sandbox_mode: String,       // "trusted" | "e2b"
    pub tools_allowed_json: String, // JSON array
    pub context_files_json: String, // JSON array
    #[index(btree)]
    pub status: String,             // "queued"|"running"|"complete"|"failed"|"budget_exceeded"
    pub result_summary: String,
    pub tokens_used: u32,
    pub latency_ms: u64,
    pub created_at: String,
    pub completed_at: String,
}
```

- [ ] **Step 2: Add create_task_dispatch reducer**

```rust
#[reducer]
pub fn create_task_dispatch(
    ctx: &ReducerContext,
    task_id: String,
    parent_task_id: Option<String>,
    intent_class: String,
    risk_level: String,
    assigned_model: String,
    effort_knob: String,
    assigned_cell: String,
    budget_max_turns: u8,
    budget_max_seconds: u32,
    fallback_model: String,
    sandbox_mode: String,
    tools_allowed_json: String,
    context_files_json: String,
) -> Result<(), String> {
    ctx.db.task_dispatches().insert(TaskDispatch {
        task_id,
        parent_task_id,
        intent_class,
        risk_level,
        assigned_model,
        effort_knob,
        assigned_cell,
        budget_max_turns,
        budget_max_seconds,
        fallback_model,
        sandbox_mode,
        tools_allowed_json,
        context_files_json,
        status: "queued".to_string(),
        result_summary: String::new(),
        tokens_used: 0,
        latency_ms: 0,
        created_at: ctx.timestamp.to_string(),
        completed_at: String::new(),
    });
    Ok(())
}
```

- [ ] **Step 3: Add update_task_dispatch_status reducer**

```rust
#[reducer]
pub fn update_task_dispatch_status(
    ctx: &ReducerContext,
    task_id: String,
    status: String,
    result_summary: String,
    tokens_used: u32,
    latency_ms: u64,
) -> Result<(), String> {
    if let Some(mut dispatch) = ctx.db.task_dispatches().task_id().find(&task_id) {
        dispatch.status = status;
        dispatch.result_summary = result_summary;
        dispatch.tokens_used = tokens_used;
        dispatch.latency_ms = latency_ms;
        dispatch.completed_at = ctx.timestamp.to_string();
        ctx.db.task_dispatches().task_id().update(dispatch);
    }
    Ok(())
}
```

- [ ] **Step 4: Compile**

Run: `cd /Users/dmcgregsauce/heiwa/apps/heiwa_hub/spacetimedb && cargo build`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs
git commit -m "feat(stdb): add task_dispatches table with create and status update reducers"
```

### Task 3: Add `rate_group_state` table

**Files:**

- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`

- [ ] **Step 1: Add RateGroupState struct and reducers**

```rust
#[table(accessor = rate_group_state, public)]
pub struct RateGroupState {
    #[primary_key]
    pub rate_group: String,
    pub turns_used: u32,
    pub turns_max: u32,
    pub window_start: String,       // ISO 8601
    pub window_seconds: u32,
    pub cooldown_until: String,     // ISO 8601, empty if not cooling down
    pub available: bool,
}

#[reducer]
pub fn upsert_rate_group_state(
    ctx: &ReducerContext,
    rate_group: String,
    turns_used: u32,
    turns_max: u32,
    window_seconds: u32,
    cooldown_until: String,
    available: bool,
) -> Result<(), String> {
    if let Some(mut existing) = ctx.db.rate_group_state().rate_group().find(&rate_group) {
        existing.turns_used = turns_used;
        existing.turns_max = turns_max;
        existing.window_start = ctx.timestamp.to_string();
        existing.window_seconds = window_seconds;
        existing.cooldown_until = cooldown_until;
        existing.available = available;
        ctx.db.rate_group_state().rate_group().update(existing);
    } else {
        ctx.db.rate_group_state().insert(RateGroupState {
            rate_group,
            turns_used,
            turns_max,
            window_start: ctx.timestamp.to_string(),
            window_seconds,
            cooldown_until,
            available,
        });
    }
    Ok(())
}
```

- [ ] **Step 2: Compile**

Run: `cd /Users/dmcgregsauce/heiwa/apps/heiwa_hub/spacetimedb && cargo build`
Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs
git commit -m "feat(stdb): add rate_group_state table with upsert reducer"
```

### Task 4: Add remaining 5 spec tables (Rust-only scaffolds)

**Files:**

- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`

> These tables complete the spec's 8-table requirement. Phase 1 adds Rust structs + basic insert/query reducers only — no Python bridge, no seeds, no CLI integration.

- [ ] **Step 1: Add ExecutionMemory table and reducer**

```rust
#[table(accessor = execution_memory, public)]
pub struct ExecutionMemory {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub task_dispatch_id: String,
    pub model_used: String,
    pub outcome: String,             // "success" | "fail" | "timeout"
    pub duration_ms: u64,
    pub error_summary: Option<String>,
    pub feedback_score: Option<f64>,
    pub created_at: String,
}

#[reducer]
pub fn insert_execution_memory(
    ctx: &ReducerContext,
    task_dispatch_id: String,
    model_used: String,
    outcome: String,
    duration_ms: u64,
    error_summary: Option<String>,
    feedback_score: Option<f64>,
) -> Result<(), String> {
    ctx.db.execution_memory().insert(ExecutionMemory {
        id: 0,
        task_dispatch_id,
        model_used,
        outcome,
        duration_ms,
        error_summary,
        feedback_score,
        created_at: ctx.timestamp.to_string(),
    });
    Ok(())
}
```

- [ ] **Step 2: Add KnowledgeEmbedding table and reducer**

```rust
#[table(accessor = knowledge_embeddings, public)]
pub struct KnowledgeEmbedding {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    pub source_type: String,
    pub source_id: String,
    pub content_hash: String,
    pub embedding_json: String,
    pub ttl_hours: u32,
    pub created_at: String,
    pub last_accessed_at: Option<String>,
}

#[reducer]
pub fn insert_knowledge_embedding(
    ctx: &ReducerContext,
    source_type: String,
    source_id: String,
    content_hash: String,
    embedding_json: String,
    ttl_hours: u32,
) -> Result<(), String> {
    ctx.db.knowledge_embeddings().insert(KnowledgeEmbedding {
        id: 0,
        source_type,
        source_id,
        content_hash,
        embedding_json,
        ttl_hours,
        created_at: ctx.timestamp.to_string(),
        last_accessed_at: None,
    });
    Ok(())
}
```

- [ ] **Step 3: Add AgentRegistry table and reducer**

```rust
#[table(accessor = agent_registry, public)]
pub struct AgentRegistryEntry {
    #[primary_key]
    pub cell_id: String,
    pub display_name: String,
    pub model_preference: Option<String>,
    pub tools_allowed_json: String,
    pub sandbox_mode: String,
    pub active: bool,
    pub created_at: String,
}

#[reducer]
pub fn upsert_agent_registry(
    ctx: &ReducerContext,
    cell_id: String,
    display_name: String,
    model_preference: Option<String>,
    tools_allowed_json: String,
    sandbox_mode: String,
    active: bool,
) -> Result<(), String> {
    if let Some(mut existing) = ctx.db.agent_registry().cell_id().find(&cell_id) {
        existing.display_name = display_name;
        existing.model_preference = model_preference;
        existing.tools_allowed_json = tools_allowed_json;
        existing.sandbox_mode = sandbox_mode;
        existing.active = active;
        ctx.db.agent_registry().cell_id().update(existing);
    } else {
        ctx.db.agent_registry().insert(AgentRegistryEntry {
            cell_id,
            display_name,
            model_preference,
            tools_allowed_json,
            sandbox_mode,
            active,
            created_at: ctx.timestamp.to_string(),
        });
    }
    Ok(())
}
```

- [ ] **Step 4: Add NodeRegistry table and reducer**

```rust
#[table(accessor = node_registry, public)]
pub struct NodeRegistryEntry {
    #[primary_key]
    pub node_id: String,
    pub hostname: String,
    pub platform: String,
    pub capabilities_json: String,
    pub last_heartbeat: Option<String>,
    pub status: String,
}

#[reducer]
pub fn upsert_node_registry(
    ctx: &ReducerContext,
    node_id: String,
    hostname: String,
    platform: String,
    capabilities_json: String,
    status: String,
) -> Result<(), String> {
    if let Some(mut existing) = ctx.db.node_registry().node_id().find(&node_id) {
        existing.hostname = hostname;
        existing.platform = platform;
        existing.capabilities_json = capabilities_json;
        existing.last_heartbeat = Some(ctx.timestamp.to_string());
        existing.status = status;
        ctx.db.node_registry().node_id().update(existing);
    } else {
        ctx.db.node_registry().insert(NodeRegistryEntry {
            node_id,
            hostname,
            platform,
            capabilities_json,
            last_heartbeat: Some(ctx.timestamp.to_string()),
            status,
        });
    }
    Ok(())
}
```

- [ ] **Step 5: Add CaptainDirective table and reducer**

```rust
#[table(accessor = captain_directives, public)]
pub struct CaptainDirective {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    pub directive_type: String,
    pub payload_json: String,
    pub priority: u8,
    #[index(btree)]
    pub status: String,
    pub created_at: String,
    pub executed_at: Option<String>,
}

#[reducer]
pub fn insert_captain_directive(
    ctx: &ReducerContext,
    directive_type: String,
    payload_json: String,
    priority: u8,
) -> Result<(), String> {
    ctx.db.captain_directives().insert(CaptainDirective {
        id: 0,
        directive_type,
        payload_json,
        priority,
        status: "pending".to_string(),
        created_at: ctx.timestamp.to_string(),
        executed_at: None,
    });
    Ok(())
}
```

- [ ] **Step 6: Compile**

Run: `cd /Users/dmcgregsauce/heiwa/apps/heiwa_hub/spacetimedb && cargo build`
Expected: Compiles without errors.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs
git commit -m "feat(stdb): add 5 scaffold tables (execution_memory, knowledge_embeddings, agent_registry, node_registry, captain_directives)"
```

### Task 5: Publish updated module to local STDB

- [ ] **Step 1: Start local SpacetimeDB if not running**

```bash
~/.local/bin/spacetime start local &
sleep 3
~/.local/bin/spacetime server list
```

Expected: `local` server shows as running on 127.0.0.1:3000.

- [ ] **Step 2: Publish the module**

```bash
cd /Users/dmcgregsauce/heiwa/apps/heiwa_hub/spacetimedb
~/.local/bin/spacetime publish --server local heiwa-hub-dev
```

Expected: Module published successfully with new tables registered.

- [ ] **Step 3: Verify tables exist**

```bash
~/.local/bin/spacetime sql --server local heiwa-hub-dev "SELECT * FROM model_tiers LIMIT 1"
~/.local/bin/spacetime sql --server local heiwa-hub-dev "SELECT * FROM task_dispatches LIMIT 1"
~/.local/bin/spacetime sql --server local heiwa-hub-dev "SELECT * FROM rate_group_state LIMIT 1"
~/.local/bin/spacetime sql --server local heiwa-hub-dev "SELECT * FROM execution_memory LIMIT 1"
~/.local/bin/spacetime sql --server local heiwa-hub-dev "SELECT * FROM knowledge_embeddings LIMIT 1"
~/.local/bin/spacetime sql --server local heiwa-hub-dev "SELECT * FROM agent_registry LIMIT 1"
~/.local/bin/spacetime sql --server local heiwa-hub-dev "SELECT * FROM node_registry LIMIT 1"
~/.local/bin/spacetime sql --server local heiwa-hub-dev "SELECT * FROM captain_directives LIMIT 1"
```

Expected: Empty result sets (all 8 tables exist but no data yet).

- [ ] **Step 4: Regenerate bindings**

```bash
cd /Users/dmcgregsauce/heiwa
bash apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh
```

Expected: Python, Rust, and TypeScript bindings regenerated.

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/ packages/heiwa_bindings/
git commit -m "feat(stdb): publish module with 8 tables (3 primary + 5 scaffolds)"
```

---

## Chunk 2: Python STDB Bridge — Model Tier API

### Task 6: Extend spacetimedb.py with model_tiers operations

**Files:**

- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- Test: `apps/heiwa_hub/tests/test_model_tiers_stdb.py`

- [ ] **Step 1: Write failing test for get_model_tiers**

Create `apps/heiwa_hub/tests/test_model_tiers_stdb.py`:

```python
"""Tests for model_tiers STDB operations."""
import pytest
from unittest.mock import patch, MagicMock
from heiwa_sdk.spacetimedb import SpacetimeDB


class TestModelTiersSTDB:
    """Test model tier STDB bridge operations."""

    def setup_method(self):
        self.stdb = SpacetimeDB.__new__(SpacetimeDB)
        self.stdb.db_identity = "test-identity"
        self.stdb.server = "local"

    @patch.object(SpacetimeDB, "query")
    def test_get_model_tiers_returns_list(self, mock_query):
        mock_query.return_value = [
            {
                "model_id": "ollama/qwen3.5:4b",
                "provider_model_id": "qwen3.5:4b",
                "provider": "ollama",
                "rate_group": "local_ollama",
                "capability_class": 2,
                "effort_knob": "thinking:on",
                "effort_level": 4,
                "cost_per_turn": 0.0,
                "max_context_tokens": 32768,
                "strengths_json": '["code_generation","research"]',
                "enabled": True,
                "last_success_rate": 1.0,
                "avg_latency_ms": 500,
                "latency_p95_ms": 1200,
            }
        ]
        result = self.stdb.get_model_tiers()
        assert len(result) == 1
        assert result[0]["model_id"] == "ollama/qwen3.5:4b"
        assert result[0]["effort_level"] == 4

    @patch.object(SpacetimeDB, "query")
    def test_get_model_tiers_by_capability_class(self, mock_query):
        mock_query.return_value = []
        result = self.stdb.get_model_tiers(capability_class=3)
        mock_query.assert_called_once()
        call_sql = mock_query.call_args[0][0]
        assert "capability_class = 3" in call_sql

    @patch.object(SpacetimeDB, "query")
    def test_get_model_tier_by_id(self, mock_query):
        mock_query.return_value = [{"model_id": "codex/gpt-4.1"}]
        result = self.stdb.get_model_tier("codex/gpt-4.1")
        assert result is not None
        assert result["model_id"] == "codex/gpt-4.1"

    @patch.object(SpacetimeDB, "query")
    def test_get_model_tier_not_found(self, mock_query):
        mock_query.return_value = []
        result = self.stdb.get_model_tier("nonexistent/model")
        assert result is None

    @patch.object(SpacetimeDB, "call")
    def test_upsert_model_tier(self, mock_call):
        self.stdb.upsert_model_tier(
            model_id="ollama/qwen3.5:4b",
            provider_model_id="qwen3.5:4b",
            provider="ollama",
            rate_group="local_ollama",
            capability_class=2,
            effort_knob="thinking:on",
            effort_level=4,
            cost_per_turn=0.0,
            max_context_tokens=32768,
            strengths=["code_generation", "research"],
            enabled=True,
        )
        mock_call.assert_called_once()
        call_args = mock_call.call_args[0]
        assert call_args[0] == "upsert_model_tier"

    @patch.object(SpacetimeDB, "call")
    def test_update_model_tier_stats(self, mock_call):
        self.stdb.update_model_tier_stats(
            model_id="ollama/qwen3.5:4b",
            success_rate=0.85,
            avg_latency_ms=600,
            latency_p95_ms=1500,
        )
        mock_call.assert_called_once()
        assert mock_call.call_args[0][0] == "update_model_tier_stats"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/dmcgregsauce/heiwa && source .venv/bin/activate && pytest apps/heiwa_hub/tests/test_model_tiers_stdb.py -v`
Expected: FAIL — `get_model_tiers`, `get_model_tier`, `upsert_model_tier`, `update_model_tier_stats` not defined.

- [ ] **Step 3: Implement model_tiers methods in spacetimedb.py**

Add to `SpacetimeDB` class in `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`:

```python
# ── Model Tiers ──────────────────────────────────────────────────────

def get_model_tiers(self, *, capability_class: int | None = None,
                    enabled_only: bool = True) -> list[dict]:
    """Query model_tiers table, optionally filtered."""
    where_clauses = []
    if enabled_only:
        where_clauses.append("enabled = true")
    if capability_class is not None:
        where_clauses.append(f"capability_class = {capability_class}")
    where = f" WHERE {' AND '.join(where_clauses)}" if where_clauses else ""
    return self.query(
        f"SELECT * FROM model_tiers{where} ORDER BY effort_level ASC, cost_per_turn ASC"
    )

def get_model_tier(self, model_id: str) -> dict | None:
    """Get a single model tier by model_id."""
    rows = self.query(
        f"SELECT * FROM model_tiers WHERE model_id = '{self._escape_sql_literal(model_id)}'"
    )
    return rows[0] if rows else None

def upsert_model_tier(
    self,
    model_id: str,
    provider_model_id: str,
    provider: str,
    rate_group: str,
    capability_class: int,
    effort_knob: str,
    effort_level: int,
    cost_per_turn: float,
    max_context_tokens: int,
    strengths: list[str],
    enabled: bool,
) -> bool:
    """Insert or update a model tier."""
    import json
    return self.call(
        "upsert_model_tier",
        model_id,
        provider_model_id,
        provider,
        rate_group,
        capability_class,
        effort_knob,
        effort_level,
        cost_per_turn,
        max_context_tokens,
        json.dumps(strengths),
        enabled,
    )

def update_model_tier_stats(
    self,
    model_id: str,
    success_rate: float,
    avg_latency_ms: int,
    latency_p95_ms: int,
) -> bool:
    """Update performance stats for a model tier (called by Captain)."""
    return self.call(
        "update_model_tier_stats",
        model_id,
        success_rate,
        avg_latency_ms,
        latency_p95_ms,
    )
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/dmcgregsauce/heiwa && pytest apps/heiwa_hub/tests/test_model_tiers_stdb.py -v`
Expected: All 6 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_sdk/heiwa_sdk/spacetimedb.py apps/heiwa_hub/tests/test_model_tiers_stdb.py
git commit -m "feat(sdk): add model_tiers STDB bridge operations with tests"
```

---

## Chunk 3: Seed Loader + Hub Boot Integration

### Task 7: Create seed loader for model_tiers

**Files:**

- Create: `packages/heiwa_sdk/heiwa_sdk/seed.py`
- Test: `apps/heiwa_hub/tests/test_seed_loader.py`
- Create: `config/seeds/model_tiers.json`

- [ ] **Step 1: Create the seed data file**

Create `config/seeds/model_tiers.json`:

```json
[
  {
    "model_id": "ollama/qwen3.5:4b",
    "provider_model_id": "qwen3.5:4b",
    "provider": "ollama",
    "rate_group": "local_ollama",
    "capability_class": 2,
    "effort_knob": "thinking:on",
    "effort_level": 4,
    "cost_per_turn": 0.0,
    "max_context_tokens": 32768,
    "strengths": ["code_generation", "research", "general"],
    "enabled": true
  },
  {
    "model_id": "ollama/qwen2.5-coder:1.5b",
    "provider_model_id": "qwen2.5-coder:1.5b",
    "provider": "ollama",
    "rate_group": "local_ollama",
    "capability_class": 1,
    "effort_knob": "thinking:off",
    "effort_level": 1,
    "cost_per_turn": 0.0,
    "max_context_tokens": 16384,
    "strengths": ["code_generation", "files"],
    "enabled": true
  },
  {
    "model_id": "ollama/qwen2.5-coder:0.5b",
    "provider_model_id": "qwen2.5-coder:0.5b",
    "provider": "ollama",
    "rate_group": "local_ollama",
    "capability_class": 1,
    "effort_knob": "thinking:off",
    "effort_level": 1,
    "cost_per_turn": 0.0,
    "max_context_tokens": 8192,
    "strengths": ["lint", "format", "simple_lookup"],
    "enabled": true
  },
  {
    "model_id": "ollama/llama3.2:3b",
    "provider_model_id": "llama3.2:3b",
    "provider": "ollama",
    "rate_group": "local_ollama",
    "capability_class": 1,
    "effort_knob": "thinking:off",
    "effort_level": 2,
    "cost_per_turn": 0.0,
    "max_context_tokens": 8192,
    "strengths": ["chat", "general"],
    "enabled": true
  },
  {
    "model_id": "gemini-cli/gemini-3-flash",
    "provider_model_id": "gemini-3-flash-preview",
    "provider": "google-gemini-cli",
    "rate_group": "google_gemini_cli",
    "capability_class": 2,
    "effort_knob": "thinking:on",
    "effort_level": 4,
    "cost_per_turn": 0.0,
    "max_context_tokens": 1000000,
    "strengths": ["research", "code_generation", "general"],
    "enabled": true
  },
  {
    "model_id": "gemini-cli/gemini-3.1-pro",
    "provider_model_id": "gemini-3-pro-preview",
    "provider": "google-gemini-cli",
    "rate_group": "google_gemini_cli",
    "capability_class": 3,
    "effort_knob": "thinking:high",
    "effort_level": 5,
    "cost_per_turn": 0.0,
    "max_context_tokens": 2000000,
    "strengths": ["research", "architecture", "long_context"],
    "enabled": true
  },
  {
    "model_id": "antigravity/gemini-3-auto",
    "provider_model_id": "gemini-3-auto",
    "provider": "google-antigravity",
    "rate_group": "google_antigravity",
    "capability_class": 2,
    "effort_knob": "thinking:always",
    "effort_level": 4,
    "cost_per_turn": 0.0,
    "max_context_tokens": 1000000,
    "strengths": ["strategy", "review", "research"],
    "enabled": true
  },
  {
    "model_id": "codex/gpt-4.1",
    "provider_model_id": "gpt-4.1",
    "provider": "codex",
    "rate_group": "openai_codex",
    "capability_class": 2,
    "effort_knob": "reasoning:medium",
    "effort_level": 3,
    "cost_per_turn": 1.08,
    "max_context_tokens": 1000000,
    "strengths": ["code_generation", "build", "refactor"],
    "enabled": true
  },
  {
    "model_id": "codex/gpt-5.4",
    "provider_model_id": "gpt-5.4",
    "provider": "codex",
    "rate_group": "openai_codex",
    "capability_class": 3,
    "effort_knob": "reasoning:xhigh",
    "effort_level": 5,
    "cost_per_turn": 1.08,
    "max_context_tokens": 1000000,
    "strengths": ["architecture", "adversarial_review", "complex_reasoning"],
    "enabled": true
  },
  {
    "model_id": "claude/sonnet-4-6",
    "provider_model_id": "claude-sonnet-4-6",
    "provider": "claude-code",
    "rate_group": "claude_code",
    "capability_class": 2,
    "effort_knob": "effort:medium",
    "effort_level": 3,
    "cost_per_turn": 0.775,
    "max_context_tokens": 200000,
    "strengths": ["code_generation", "review", "research"],
    "enabled": true
  },
  {
    "model_id": "claude/opus-4-6",
    "provider_model_id": "claude-opus-4-6",
    "provider": "claude-code",
    "rate_group": "claude_code",
    "capability_class": 3,
    "effort_knob": "effort:high",
    "effort_level": 5,
    "cost_per_turn": 0.775,
    "max_context_tokens": 200000,
    "strengths": ["architecture", "adversarial_review", "complex_reasoning", "code_review"],
    "enabled": true
  },
  {
    "model_id": "ollama/qwen3-embedding:0.6b",
    "provider_model_id": "qwen3-embedding:0.6b",
    "provider": "ollama",
    "rate_group": "local_ollama",
    "capability_class": 1,
    "effort_knob": "n/a",
    "effort_level": 1,
    "cost_per_turn": 0.0,
    "max_context_tokens": 8192,
    "strengths": ["embeddings"],
    "enabled": true
  }
]
```

- [ ] **Step 2: Write failing test for seed loader**

Create `apps/heiwa_hub/tests/test_seed_loader.py`:

```python
"""Tests for STDB seed loader."""
import json
import pytest
from pathlib import Path
from unittest.mock import patch, MagicMock, call
from heiwa_sdk.seed import SeedLoader


class TestSeedLoader:
    """Test seed loading from JSON to STDB."""

    def test_load_model_tiers_seed_file(self):
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        assert seed_path.exists(), f"Seed file not found: {seed_path}"
        with open(seed_path) as f:
            tiers = json.load(f)
        assert len(tiers) >= 10
        for tier in tiers:
            assert "model_id" in tier
            assert "provider" in tier
            assert "effort_level" in tier
            assert 1 <= tier["effort_level"] <= 5

    @patch("heiwa_sdk.seed.SpacetimeDB")
    def test_seed_model_tiers_calls_upsert(self, mock_stdb_cls):
        mock_stdb = MagicMock()
        mock_stdb_cls.return_value = mock_stdb
        mock_stdb.get_model_tiers.return_value = []  # empty = needs seeding

        loader = SeedLoader(stdb=mock_stdb)
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        loader.seed_model_tiers(seed_path)

        assert mock_stdb.upsert_model_tier.call_count >= 10

    @patch("heiwa_sdk.seed.SpacetimeDB")
    def test_seed_skips_if_already_populated(self, mock_stdb_cls):
        mock_stdb = MagicMock()
        mock_stdb_cls.return_value = mock_stdb
        mock_stdb.get_model_tiers.return_value = [{"model_id": "existing"}]

        loader = SeedLoader(stdb=mock_stdb)
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        loader.seed_model_tiers(seed_path)

        mock_stdb.upsert_model_tier.assert_not_called()
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd /Users/dmcgregsauce/heiwa && pytest apps/heiwa_hub/tests/test_seed_loader.py -v`
Expected: FAIL — `heiwa_sdk.seed` module not found.

- [ ] **Step 4: Implement seed.py**

Create `packages/heiwa_sdk/heiwa_sdk/seed.py`:

```python
"""Seed loader: bootstrap STDB tables from checked-in JSON files."""
from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from heiwa_sdk.spacetimedb import SpacetimeDB

logger = logging.getLogger(__name__)


class SeedLoader:
    """Loads seed data into STDB tables on first boot."""

    def __init__(self, stdb: SpacetimeDB) -> None:
        self.stdb = stdb

    def seed_model_tiers(self, seed_path: Path) -> None:
        """Seed model_tiers from JSON. Skips if table already has data."""
        existing = self.stdb.get_model_tiers(enabled_only=False)
        if existing:
            logger.info("model_tiers already populated (%d rows), skipping seed.", len(existing))
            return

        with open(seed_path) as f:
            tiers = json.load(f)

        for tier in tiers:
            self.stdb.upsert_model_tier(
                model_id=tier["model_id"],
                provider_model_id=tier["provider_model_id"],
                provider=tier["provider"],
                rate_group=tier["rate_group"],
                capability_class=tier["capability_class"],
                effort_knob=tier["effort_knob"],
                effort_level=tier["effort_level"],
                cost_per_turn=tier["cost_per_turn"],
                max_context_tokens=tier["max_context_tokens"],
                strengths=tier["strengths"],
                enabled=tier["enabled"],
            )

        logger.info("Seeded %d model tiers from %s", len(tiers), seed_path.name)

    def seed_rate_groups(self, router_path: Path) -> None:
        """Seed rate_group_state from ai_router.json rate_limits section."""
        with open(router_path) as f:
            router = json.load(f)

        for group, limits in router.get("rate_limits", {}).items():
            self.stdb.call(
                "upsert_rate_group_state",
                group,
                0,  # turns_used
                limits["max_turns"],
                limits["window_sec"],
                "",  # cooldown_until (empty = not cooling)
                True,  # available
            )

        logger.info("Seeded %d rate groups.", len(router.get("rate_limits", {})))
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd /Users/dmcgregsauce/heiwa && pytest apps/heiwa_hub/tests/test_seed_loader.py -v`
Expected: All 3 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add config/seeds/model_tiers.json packages/heiwa_sdk/heiwa_sdk/seed.py apps/heiwa_hub/tests/test_seed_loader.py
git commit -m "feat(sdk): add seed loader for model_tiers and rate_group_state"
```

### Task 8: Integrate seed loading into hub boot sequence

**Files:**

- Modify: `apps/heiwa_hub/main.py`

- [ ] **Step 1: Add seed loading after STDB initialization**

In `main.py`, after the database initialization block, add:

```python
# Seed STDB tables from checked-in config (first boot only)
if settings.state_backend == "spacetimedb":
    from heiwa_sdk.seed import SeedLoader
    from pathlib import Path
    seed_loader = SeedLoader(stdb=db.stdb)
    repo_root = Path(__file__).parents[2]
    seed_loader.seed_model_tiers(repo_root / "config" / "seeds" / "model_tiers.json")
    seed_loader.seed_rate_groups(repo_root / "config" / "swarm" / "ai_router.json")
```

- [ ] **Step 2: Commit**

```bash
git add apps/heiwa_hub/main.py
git commit -m "feat(hub): seed model_tiers and rate_groups on STDB boot"
```

---

## Chunk 4: ComputeRouter Migration — Read from STDB

### Task 9: Update ComputeRouter to read model_tiers from STDB

**Files:**

- Modify: `packages/heiwa_cognition/heiwa_cognition/router.py`
- Test: `apps/heiwa_hub/tests/test_compute_router_stdb.py`

- [ ] **Step 1: Write failing test for STDB-backed routing**

Create `apps/heiwa_hub/tests/test_compute_router_stdb.py`:

```python
"""Tests for ComputeRouter reading from STDB model_tiers."""
import pytest
from unittest.mock import MagicMock, patch
from heiwa_cognition.router import ComputeRouter


class TestComputeRouterSTDB:
    """Test that router reads model tiers from STDB when available."""

    def _mock_tiers(self):
        return [
            {
                "model_id": "ollama/qwen3.5:4b",
                "provider": "ollama",
                "rate_group": "local_ollama",
                "capability_class": 2,
                "effort_knob": "thinking:on",
                "effort_level": 4,
                "strengths_json": '["code_generation","research","general"]',
                "enabled": True,
                "cost_per_turn": 0.0,
                "last_success_rate": 1.0,
            },
            {
                "model_id": "gemini-cli/gemini-3-flash",
                "provider": "google-gemini-cli",
                "rate_group": "google_gemini_cli",
                "capability_class": 2,
                "effort_knob": "thinking:on",
                "effort_level": 4,
                "strengths_json": '["research","code_generation"]',
                "enabled": True,
                "cost_per_turn": 0.0,
                "last_success_rate": 0.95,
            },
        ]

    def test_router_uses_stdb_tiers_for_model_selection(self):
        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = self._mock_tiers()
        router = ComputeRouter(stdb=mock_stdb)
        route = router.route("audit", "low")
        assert route.target_model is not None

    def test_router_picks_cheapest_capable_model(self):
        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = self._mock_tiers()
        router = ComputeRouter(stdb=mock_stdb)
        route = router.route("audit", "low")
        # Audit is light — should pick cheapest (ollama, cost=0.0)
        assert "ollama" in route.target_model

    def test_router_falls_back_to_json_if_no_stdb(self):
        router = ComputeRouter(stdb=None)
        route = router.route("audit", "low")
        assert route.target_model is not None

    def test_route_includes_effort_knob(self):
        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = self._mock_tiers()
        router = ComputeRouter(stdb=mock_stdb)
        route = router.route("research", "low")
        assert route.effort_knob is not None
        assert route.effort_knob != ""
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/dmcgregsauce/heiwa && pytest apps/heiwa_hub/tests/test_compute_router_stdb.py -v`
Expected: FAIL — ComputeRouter doesn't accept `stdb` parameter, no `effort_knob` on ComputeRoute.

- [ ] **Step 3: Add effort_knob to ComputeRoute dataclass**

In `packages/heiwa_cognition/heiwa_cognition/router.py`, find the `ComputeRoute` class and add:

```python
effort_knob: str = ""  # provider-specific effort setting
```

- [ ] **Step 4: Update ComputeRouter.**init** to accept stdb**

Modify the constructor to accept an optional STDB instance:

```python
def __init__(self, router_path: Path | None = None, stdb=None) -> None:
    root = Path(__file__).resolve().parents[3]
    self.router_path = router_path or root / "config" / "swarm" / "ai_router.json"
    self.router_config = self._load_router()
    self.registry = self.router_config.get("models", {}).get("registry", {})
    self._rotation = (
        self.router_config
        .get("routing_policy", {})
        .get("provider_rotation", {})
        .get("premium_remote", [])
    )
    self._intent_rotations = (
        self.router_config
        .get("routing_policy", {})
        .get("provider_rotation", {})
        .get("by_intent", {})
    )
    self._providers = self.router_config.get("providers", {})

    # STDB model tier support (falls back to JSON config when None)
    self._stdb = stdb
    self._model_tiers: list[dict] | None = None
    if stdb:
        try:
            self._model_tiers = stdb.get_model_tiers()
            logger.info("ComputeRouter: loaded %d model tiers from STDB", len(self._model_tiers))
        except Exception as e:
            logger.warning("ComputeRouter: STDB unavailable (%s), falling back to JSON", e)
```

- [ ] **Step 5: Add _select_model_from_tiers method**

```python
def _select_model_from_tiers(self, intent_class: str, risk_level: str) -> tuple[str, str] | None:
    """Select cheapest capable model from STDB tiers. Returns (model_id, effort_knob) or None."""
    if not self._model_tiers:
        return None

    import json

    # Determine minimum capability class from risk
    min_class = {"low": 1, "medium": 2, "high": 3, "critical": 3}.get(risk_level, 2)

    candidates = []
    for tier in self._model_tiers:
        if tier["capability_class"] < min_class:
            continue
        if not tier.get("enabled", True):
            continue
        strengths = json.loads(tier.get("strengths_json", "[]"))
        # Prefer models whose strengths include this intent
        intent_match = intent_class in strengths or "general" in strengths
        candidates.append((tier, intent_match))

    if not candidates:
        return None

    # Sort: intent match first, then cheapest, then lowest effort
    candidates.sort(key=lambda c: (
        not c[1],                    # intent matches first
        c[0]["cost_per_turn"],       # cheapest first
        c[0]["effort_level"],        # lowest effort first
    ))

    best = candidates[0][0]
    return best["model_id"], best["effort_knob"]
```

- [ ] **Step 6: Wire _select_model_from_tiers into route()**

In the `route()` method, before the existing routing logic, add an STDB-first path:

```python
# STDB-first: override model selection from tiers if available
# (does NOT short-circuit — route still flows through identity hint,
#  feedback preference, and cascade logic below)
tier_result = self._select_model_from_tiers(intent_class, risk_level)
stdb_override = None
if tier_result:
    stdb_override = tier_result  # (model_id, effort_knob)

# ... existing routing logic runs normally ...
# After _route_inner + post-processing, apply STDB override:
# (insert after existing route = self._route_inner(...) and post-processing)
if stdb_override:
    route.target_model = stdb_override[0]
    route.effort_knob = stdb_override[1]
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cd /Users/dmcgregsauce/heiwa && pytest apps/heiwa_hub/tests/test_compute_router_stdb.py -v`
Expected: All 4 tests PASS.

- [ ] **Step 8: Run existing router tests to verify no regressions**

Run: `cd /Users/dmcgregsauce/heiwa && pytest apps/heiwa_hub/tests/test_compute_router.py -v`
Expected: All existing tests PASS (fallback to JSON still works).

- [ ] **Step 9: Commit**

```bash
git add packages/heiwa_cognition/heiwa_cognition/router.py apps/heiwa_hub/tests/test_compute_router_stdb.py
git commit -m "feat(router): ComputeRouter reads model_tiers from STDB with JSON fallback"
```

---

## Chunk 5: CLI Commands — `heiwa start` + `heiwa inspect`

### Task 10: Add `heiwa inspect` command

**Files:**

- Modify: `packages/heiwa_cli/heiwa_cli/commands.py`
- Test: `apps/heiwa_hub/tests/test_cli_inspect.py`

- [ ] **Step 1: Write failing test**

Create `apps/heiwa_hub/tests/test_cli_inspect.py`:

```python
"""Tests for heiwa inspect CLI command."""
import pytest
from unittest.mock import AsyncMock, MagicMock, patch


class TestInspectCommand:
    """Test the /inspect CLI command."""

    @pytest.mark.asyncio
    @patch("heiwa_cli.commands.SpacetimeDB")
    async def test_inspect_model_tiers(self, mock_stdb_cls):
        from heiwa_cli.commands import cmd_inspect
        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = [
            {"model_id": "ollama/qwen3.5:4b", "effort_level": 4, "enabled": True}
        ]
        ctx = MagicMock()
        ctx.stdb = mock_stdb
        await cmd_inspect(ctx, "model_tiers")
        mock_stdb.get_model_tiers.assert_called_once()

    @pytest.mark.asyncio
    async def test_inspect_unknown_table(self):
        from heiwa_cli.commands import cmd_inspect
        ctx = MagicMock()
        ctx.stdb = MagicMock()
        # Should not raise, just print error
        await cmd_inspect(ctx, "nonexistent_table")
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/dmcgregsauce/heiwa && pytest apps/heiwa_hub/tests/test_cli_inspect.py -v`
Expected: FAIL — `cmd_inspect` not defined.

- [ ] **Step 3: Implement cmd_inspect**

Add to `packages/heiwa_cli/heiwa_cli/commands.py`:

```python
@command("/inspect", "Inspect an STDB table (model_tiers, rate_group_state, task_dispatches)")
async def cmd_inspect(ctx: CLIContext, args: str = "") -> None:
    table = args.strip() or "model_tiers"
    stdb = getattr(ctx, "stdb", None)
    if not stdb:
        print("  STDB not connected. Run with HEIWA_STATE_BACKEND=spacetimedb.")
        return

    inspectors = {
        "model_tiers": lambda: stdb.get_model_tiers(enabled_only=False),
        "rate_group_state": lambda: stdb.query("SELECT * FROM rate_group_state"),
        "task_dispatches": lambda: stdb.query(
            "SELECT task_id, intent_class, status, assigned_model, effort_knob FROM task_dispatches ORDER BY created_at DESC LIMIT 20"
        ),
    }

    if table not in inspectors:
        print(f"  Unknown table: {table}")
        print(f"  Available: {', '.join(inspectors.keys())}")
        return

    rows = inspectors[table]()
    if not rows:
        print(f"  {table}: (empty)")
        return

    # Print as formatted table
    keys = list(rows[0].keys())
    print(f"\n  {table} ({len(rows)} rows):")
    print(f"  {'  '.join(k[:20].ljust(20) for k in keys[:6])}")
    print(f"  {'─' * 120}")
    for row in rows:
        vals = [str(row.get(k, ""))[:20].ljust(20) for k in keys[:6]]
        print(f"  {'  '.join(vals)}")
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/dmcgregsauce/heiwa && pytest apps/heiwa_hub/tests/test_cli_inspect.py -v`
Expected: All 2 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_cli/heiwa_cli/commands.py apps/heiwa_hub/tests/test_cli_inspect.py
git commit -m "feat(cli): add /inspect command for STDB table debugging"
```

### Task 11: Add `heiwa start` command

**Files:**

- Modify: `packages/heiwa_cli/heiwa_cli/commands.py`
- Test: `apps/heiwa_hub/tests/test_cli_start.py`

- [ ] **Step 1: Write failing test**

Create `apps/heiwa_hub/tests/test_cli_start.py`:

```python
"""Tests for heiwa start CLI command."""
import pytest
from unittest.mock import AsyncMock, MagicMock, patch


class TestStartCommand:
    """Test the /start CLI command."""

    @pytest.mark.asyncio
    @patch("heiwa_cli.commands.subprocess")
    async def test_start_sets_stdb_backend(self, mock_subprocess):
        from heiwa_cli.commands import cmd_start
        import os
        ctx = MagicMock()
        mock_subprocess.Popen.return_value = MagicMock(pid=12345)
        with patch.dict(os.environ, {}, clear=False):
            await cmd_start(ctx)
        assert os.environ.get("HEIWA_STATE_BACKEND") == "spacetimedb"

    @pytest.mark.asyncio
    @patch("heiwa_cli.commands.subprocess")
    async def test_start_launches_hub_process(self, mock_subprocess):
        from heiwa_cli.commands import cmd_start
        ctx = MagicMock()
        mock_subprocess.Popen.return_value = MagicMock(pid=12345)
        await cmd_start(ctx)
        assert mock_subprocess.Popen.called
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/dmcgregsauce/heiwa && pytest apps/heiwa_hub/tests/test_cli_start.py -v`
Expected: FAIL — `cmd_start` not defined.

- [ ] **Step 3: Implement cmd_start**

```python
@command("/start", "Start Heiwa hub locally (STDB + agents + HTTP server)")
async def cmd_start(ctx: CLIContext, args: str = "") -> None:
    import asyncio
    import subprocess
    import os
    from pathlib import Path

    repo_root = Path(__file__).parents[3]

    # Step 1: Start local STDB if not running
    spacetime = os.path.expanduser("~/.local/bin/spacetime")
    if os.path.exists(spacetime):
        print("  Starting local SpacetimeDB...")
        subprocess.Popen(
            [spacetime, "start", "local"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        await asyncio.sleep(2)

    # Step 2: Set env for STDB backend
    os.environ.setdefault("HEIWA_STATE_BACKEND", "spacetimedb")
    os.environ.setdefault("STDB_SERVER", "local")
    os.environ.setdefault("OLLAMA_BASE_URL", "http://localhost:11434")

    # Step 3: Start hub
    print("  Starting Heiwa Hub on port 8080...")
    hub_proc = subprocess.Popen(
        [
            str(repo_root / ".venv" / "bin" / "python"), "-m", "apps.heiwa_hub.main",
        ],
        cwd=str(repo_root),
        env={**os.environ, "PYTHONPATH": ":".join([
            str(repo_root / "packages" / p) for p in
            ["heiwa_cli", "heiwa_cognition", "heiwa_sdk", "heiwa_protocol", "heiwa_identity", "heiwa_ui"]
        ] + [str(repo_root / "apps")])},
    )
    print(f"  Hub started (PID {hub_proc.pid}). Health: http://localhost:8080/health")
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/dmcgregsauce/heiwa && pytest apps/heiwa_hub/tests/test_cli_start.py -v`
Expected: All 2 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_cli/heiwa_cli/commands.py apps/heiwa_hub/tests/test_cli_start.py
git commit -m "feat(cli): add /start command for one-command local boot"
```

---

## Chunk 6: Integration Test — Full Pipeline

### Task 12: End-to-end integration test

**Files:**

- Test: `apps/heiwa_hub/tests/test_phase1_integration.py`

- [ ] **Step 1: Write integration test**

```python
"""Phase 1 integration test: seed → route → dispatch with effort knobs."""
import json
import pytest
from pathlib import Path
from unittest.mock import MagicMock, patch


class TestPhase1Integration:
    """Verify the full Phase 1 pipeline: seed → STDB → router → dispatch."""

    def _seed_mock_stdb(self):
        """Create a mock STDB with seeded model tiers."""
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        with open(seed_path) as f:
            tiers = json.load(f)

        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = [
            {**t, "strengths_json": json.dumps(t["strengths"]),
             "last_success_rate": 1.0, "avg_latency_ms": 0, "latency_p95_ms": 0}
            for t in tiers
        ]
        return mock_stdb

    def test_audit_routes_to_cheapest_local_model(self):
        from heiwa_cognition.router import ComputeRouter
        stdb = self._seed_mock_stdb()
        router = ComputeRouter(stdb=stdb)
        route = router.route("audit", "low")
        # Audit/low should pick a cheap local model
        assert route.target_model is not None
        assert route.effort_knob != ""
        assert "ollama" in route.target_model or route.compute_class == 1

    def test_research_routes_to_capable_model_with_thinking(self):
        from heiwa_cognition.router import ComputeRouter
        stdb = self._seed_mock_stdb()
        router = ComputeRouter(stdb=stdb)
        route = router.route("research", "medium")
        assert route.target_model is not None
        assert route.effort_knob != ""
        # Research should get thinking enabled
        assert "thinking" in route.effort_knob or "effort" in route.effort_knob

    def test_build_routes_with_code_gen_strength(self):
        from heiwa_cognition.router import ComputeRouter
        stdb = self._seed_mock_stdb()
        router = ComputeRouter(stdb=stdb)
        route = router.route("build", "medium")
        assert route.target_model is not None

    def test_seed_file_has_all_required_fields(self):
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        with open(seed_path) as f:
            tiers = json.load(f)

        required = ["model_id", "provider_model_id", "provider", "rate_group",
                     "capability_class", "effort_knob", "effort_level",
                     "cost_per_turn", "max_context_tokens", "strengths", "enabled"]
        for tier in tiers:
            for field in required:
                assert field in tier, f"Missing {field} in tier {tier.get('model_id', '?')}"
            assert 1 <= tier["effort_level"] <= 5
            assert tier["capability_class"] in (1, 2, 3)

    def test_all_providers_represented_in_seed(self):
        seed_path = Path(__file__).parents[3] / "config" / "seeds" / "model_tiers.json"
        with open(seed_path) as f:
            tiers = json.load(f)
        providers = {t["provider"] for t in tiers}
        expected = {"ollama", "google-gemini-cli", "google-antigravity", "codex", "claude-code"}
        assert expected.issubset(providers), f"Missing providers: {expected - providers}"
```

- [ ] **Step 2: Run full test suite**

Run: `cd /Users/dmcgregsauce/heiwa && pytest apps/heiwa_hub/tests/ -v --tb=short`
Expected: All new tests PASS, all existing tests PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/heiwa_hub/tests/test_phase1_integration.py
git commit -m "test: add Phase 1 integration tests for seed → route → dispatch pipeline"
```

- [ ] **Step 4: Run full test suite one final time**

Run: `cd /Users/dmcgregsauce/heiwa && pytest -v`
Expected: All tests PASS. Phase 1 is complete.

- [ ] **Step 5: Final commit with all Phase 1 changes**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs \
      packages/heiwa_bindings/ \
      packages/heiwa_sdk/heiwa_sdk/spacetimedb.py \
      packages/heiwa_sdk/heiwa_sdk/seed.py \
      packages/heiwa_cognition/heiwa_cognition/router.py \
      packages/heiwa_cli/heiwa_cli/commands.py \
      apps/heiwa_hub/main.py \
      apps/heiwa_hub/tests/ \
      config/seeds/model_tiers.json
git commit -m "feat: Phase 1 complete — STDB foundation + model tier matrix

- 8 new STDB tables: model_tiers, task_dispatches, rate_group_state + 5 Rust-only scaffolds
- Seed loader bootstraps from config/seeds/model_tiers.json
- ComputeRouter reads from STDB with JSON fallback
- CLI: /inspect for STDB debugging, /start for local boot
- 12 provider-model configurations with effort knobs
- Integration tests verify full pipeline"
```
