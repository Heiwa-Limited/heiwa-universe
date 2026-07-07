# Heiwa Agent Memory & Railway Deployment Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Heiwa Agent persistent cross-session memory via SpacetimeDB, rename Captain → Heiwa Agent, split the Antigravity rate group, and harden the Railway health check.

**Architecture:** Three new STDB tables (`captain_messages`, `captain_summaries`, `captain_focus`) with 6 reducers. Python `AgentMemory` service wraps STDB calls. Heiwa Agent runtime adds a memory loop (store → load → retrieve → reason → respond → focus). Antigravity rate group splits into `antigravity_flash` (20/hr, Heiwa Agent exclusive) and `antigravity_pro` (15/hr, cascade). Health check verifies STDB connectivity with 2s timeout.

**Tech Stack:** Rust (SpacetimeDB module), Python 3.11+ (asyncio agents), STDB CLI bridge (`spacetime call`/`spacetime sql`)

**Spec:** `docs/superpowers/specs/2026-03-17-heiwa-agent-memory-railway-deployment-design.md`

---

## File Structure

### New Files

| File                                           | Responsibility                                                            |
| ---------------------------------------------- | ------------------------------------------------------------------------- |
| `packages/heiwa_sdk/heiwa_sdk/agent_memory.py` | Python bridge for captain_messages/summaries/focus STDB operations        |
| `apps/heiwa_hub/agents/heiwa_agent.py`         | Renamed from `captain.py` — adds memory loop, compression, boot hydration |
| `apps/heiwa_hub/tests/test_agent_memory.py`    | Unit tests for AgentMemory service                                        |
| `apps/heiwa_hub/tests/test_heiwa_agent.py`     | Unit tests for HeiwaAgent runtime (memory loop, compression, boot)        |

### Modified Files

| File                                                 | Changes                                                      |
| ---------------------------------------------------- | ------------------------------------------------------------ |
| `apps/heiwa_hub/spacetimedb/src/lib.rs`              | Add 3 tables + 6 reducers (~120 lines at end of file)        |
| `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`        | Add 6 STDB bridge methods for captain memory tables          |
| `apps/heiwa_hub/main.py`                             | Update import `CaptainAgent` → `HeiwaAgent`                  |
| `apps/heiwa_hub/tests/test_phase3_integration.py`    | Update import path                                           |
| `packages/heiwa_protocol/heiwa_protocol/protocol.py` | Add `HEIWA_AGENT_DM` subject (keep `CAPTAIN_DM` as alias)    |
| `apps/heiwa_hub/mcp_server.py:216-225`               | Harden `/health` with STDB ping + 2s timeout                 |
| `config/swarm/ai_router.json`                        | Split `google_antigravity` rate group into flash + pro       |
| `config/seeds/model_tiers.json`                      | Replace `antigravity/gemini-3-auto` with flash + pro entries |

---

## Chunk 1: STDB Schema + Memory Service

### Task 1: Add STDB Tables (Rust)

**Files:**

- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs:1984-1988` (append after last line)

- [ ] **Step 1: Add `captain_messages` table**

Append after line 1983 (after `prune_execution_memory` reducer):

```rust
// ── Captain Memory (Heiwa Agent persistent conversation) ───────

#[table(accessor = captain_messages, public)]
pub struct CaptainMessage {
    #[primary_key]
    pub message_id: String,
    #[index(btree)]
    pub session_id: String,
    pub role: String,
    pub content: String,
    #[index(btree)]
    pub timestamp: u64,
    pub source: String,
    pub compressed: bool,
}
```

- [ ] **Step 2: Add `captain_summaries` table**

```rust
#[table(accessor = captain_summaries, public)]
pub struct CaptainSummary {
    #[primary_key]
    pub summary_id: String,
    #[index(btree)]
    pub summary_type: String,
    pub content: String,
    pub message_range_start: u64,
    pub message_range_end: u64,
    pub messages_compressed: u32,
    pub created_at: u64,
}
```

- [ ] **Step 3: Add `captain_focus` table**

```rust
#[table(accessor = captain_focus, public)]
pub struct CaptainFocus {
    #[primary_key]
    pub focus_id: String,
    pub topic: String,
    pub context_json: String,
    #[index(btree)]
    pub priority: u8,
    pub created_at: u64,
    pub resolved_at: u64,
}
```

- [ ] **Step 4: Add `insert_captain_message` reducer**

```rust
#[reducer]
pub fn insert_captain_message(
    ctx: &ReducerContext,
    message_id: String,
    session_id: String,
    role: String,
    content: String,
    timestamp: u64,
    source: String,
) -> Result<(), String> {
    ctx.db.captain_messages().insert(CaptainMessage {
        message_id,
        session_id,
        role,
        content,
        timestamp,
        source,
        compressed: false,
    });
    Ok(())
}
```

- [ ] **Step 5: Add `mark_messages_compressed` reducer**

```rust
#[reducer]
pub fn mark_messages_compressed(
    ctx: &ReducerContext,
    session_id: String,
    before_timestamp: u64,
) -> Result<(), String> {
    let msgs: Vec<CaptainMessage> = ctx
        .db
        .captain_messages()
        .iter()
        .filter(|m| m.session_id == session_id && m.timestamp < before_timestamp && !m.compressed)
        .collect();
    for mut msg in msgs {
        msg.compressed = true;
        ctx.db.captain_messages().message_id().update(msg);
    }
    Ok(())
}
```

- [ ] **Step 6: Add `insert_captain_summary` reducer**

```rust
#[reducer]
pub fn insert_captain_summary(
    ctx: &ReducerContext,
    summary_id: String,
    summary_type: String,
    content: String,
    range_start: u64,
    range_end: u64,
    messages_compressed: u32,
) -> Result<(), String> {
    ctx.db.captain_summaries().insert(CaptainSummary {
        summary_id,
        summary_type,
        content,
        message_range_start: range_start,
        message_range_end: range_end,
        messages_compressed,
        created_at: ctx.timestamp.to_micros_since_unix_epoch() / 1000,
    });
    Ok(())
}
```

- [ ] **Step 7: Add `upsert_captain_focus` and `resolve_captain_focus` reducers**

```rust
#[reducer]
pub fn upsert_captain_focus(
    ctx: &ReducerContext,
    focus_id: String,
    topic: String,
    context_json: String,
    priority: u8,
) -> Result<(), String> {
    if let Some(mut existing) = ctx.db.captain_focus().focus_id().find(&focus_id) {
        existing.topic = topic;
        existing.context_json = context_json;
        existing.priority = priority;
        ctx.db.captain_focus().focus_id().update(existing);
    } else {
        ctx.db.captain_focus().insert(CaptainFocus {
            focus_id,
            topic,
            context_json,
            priority,
            created_at: ctx.timestamp.to_micros_since_unix_epoch() / 1000,
            resolved_at: 0,
        });
    }
    Ok(())
}

#[reducer]
pub fn resolve_captain_focus(
    ctx: &ReducerContext,
    focus_id: String,
    resolved_at: u64,
) -> Result<(), String> {
    let mut focus = ctx
        .db
        .captain_focus()
        .focus_id()
        .find(&focus_id)
        .ok_or("Focus not found")?;
    focus.resolved_at = resolved_at;
    ctx.db.captain_focus().focus_id().update(focus);
    Ok(())
}
```

- [ ] **Step 8: Add `prune_captain_messages` reducer**

```rust
#[reducer]
pub fn prune_captain_messages(
    ctx: &ReducerContext,
    before_timestamp: u64,
) -> Result<(), String> {
    let old: Vec<String> = ctx
        .db
        .captain_messages()
        .iter()
        .filter(|m| m.compressed && m.timestamp < before_timestamp)
        .map(|m| m.message_id.clone())
        .collect();
    for id in old {
        ctx.db.captain_messages().message_id().delete(&id);
    }
    Ok(())
}
```

- [ ] **Step 9: Verify Rust compiles**

Run: `cd apps/heiwa_hub/spacetimedb && spacetime build 2>&1 | tail -5`
Expected: Build succeeds with no errors

- [ ] **Step 10: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs
git commit -m "feat(stdb): add captain_messages, captain_summaries, captain_focus tables and reducers"
```

---

### Task 2: STDB Python Bridge Methods

**Files:**

- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` (append after `insert_captain_directive` method, ~line 957)

- [ ] **Step 1: Write the failing test for bridge methods**

Create: `apps/heiwa_hub/tests/test_agent_memory.py`

```python
"""Tests for AgentMemory STDB bridge and service layer."""
import pytest
from unittest.mock import MagicMock, patch


class TestSpacetimeDBBridge:
    """Test the STDB bridge methods for captain memory tables."""

    def test_insert_captain_message_calls_reducer(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.call = MagicMock(return_value=True)

        result = stdb.insert_captain_message(
            message_id="msg-1",
            session_id="sess-1",
            role="operator",
            content="hello",
            timestamp=1710000000000,
            source="discord_dm",
        )
        assert result is True
        stdb.call.assert_called_once_with(
            "insert_captain_message",
            "msg-1", "sess-1", "operator", "hello", 1710000000000, "discord_dm",
        )

    def test_get_uncompressed_messages_queries(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.query = MagicMock(return_value=[
            {"message_id": "m1", "role": "operator", "content": "hi", "timestamp": 100},
        ])

        result = stdb.get_uncompressed_messages(session_id="sess-1", limit=50)
        assert len(result) == 1
        assert result[0]["message_id"] == "m1"
        stdb.query.assert_called_once()
        assert "compressed = false" in stdb.query.call_args[0][0]

    def test_mark_messages_compressed_calls_reducer(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.call = MagicMock(return_value=True)

        result = stdb.mark_messages_compressed(session_id="sess-1", before_timestamp=999)
        assert result is True
        stdb.call.assert_called_once_with("mark_messages_compressed", "sess-1", 999)

    def test_insert_captain_summary_calls_reducer(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.call = MagicMock(return_value=True)

        result = stdb.insert_captain_summary(
            summary_id="sum-1",
            summary_type="rolling",
            content="summary text",
            range_start=100,
            range_end=500,
            messages_compressed=10,
        )
        assert result is True
        stdb.call.assert_called_once_with(
            "insert_captain_summary",
            "sum-1", "rolling", "summary text", 100, 500, 10,
        )

    def test_get_recent_summaries_queries(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.query = MagicMock(return_value=[
            {"summary_id": "s1", "content": "day recap", "created_at": 200},
        ])

        result = stdb.get_recent_summaries(limit=3)
        assert len(result) == 1
        stdb.query.assert_called_once()

    def test_get_active_focuses_queries(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.query = MagicMock(return_value=[
            {"focus_id": "f1", "topic": "deployment", "priority": 3},
        ])

        result = stdb.get_active_focuses()
        assert len(result) == 1
        assert result[0]["topic"] == "deployment"
        stdb.query.assert_called_once()
        assert "resolved_at = 0" in stdb.query.call_args[0][0]
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pytest apps/heiwa_hub/tests/test_agent_memory.py -v`
Expected: FAIL — methods don't exist on SpacetimeDB yet

- [ ] **Step 3: Implement bridge methods in spacetimedb.py**

Append after `insert_captain_directive` (~line 957):

```python
    # ── Captain Memory (Heiwa Agent conversation persistence) ──────

    def insert_captain_message(
        self,
        message_id: str,
        session_id: str,
        role: str,
        content: str,
        timestamp: int,
        source: str,
    ) -> bool:
        return self.call(
            "insert_captain_message",
            message_id, session_id, role, content, timestamp, source,
        )

    def get_uncompressed_messages(
        self,
        session_id: str | None = None,
        limit: int = 100,
    ) -> list[dict[str, Any]]:
        query = "SELECT * FROM captain_messages WHERE compressed = false"
        if session_id:
            query += f" AND session_id = '{self._escape_sql_literal(session_id)}'"
        query += f" LIMIT {int(limit)}"
        rows = self.query(query)
        return sorted(rows, key=lambda r: r.get("timestamp", 0))

    def mark_messages_compressed(self, session_id: str, before_timestamp: int) -> bool:
        return self.call("mark_messages_compressed", session_id, before_timestamp)

    def insert_captain_summary(
        self,
        summary_id: str,
        summary_type: str,
        content: str,
        range_start: int,
        range_end: int,
        messages_compressed: int,
    ) -> bool:
        return self.call(
            "insert_captain_summary",
            summary_id, summary_type, content, range_start, range_end, messages_compressed,
        )

    def get_recent_summaries(self, limit: int = 5) -> list[dict[str, Any]]:
        rows = self.query("SELECT * FROM captain_summaries")
        return sorted(rows, key=lambda r: r.get("created_at", 0), reverse=True)[:limit]

    def upsert_captain_focus(
        self,
        focus_id: str,
        topic: str,
        context_json: str,
        priority: int,
    ) -> bool:
        return self.call("upsert_captain_focus", focus_id, topic, context_json, int(priority))

    def resolve_captain_focus(self, focus_id: str, resolved_at: int) -> bool:
        return self.call("resolve_captain_focus", focus_id, resolved_at)

    def get_active_focuses(self) -> list[dict[str, Any]]:
        rows = self.query("SELECT * FROM captain_focus WHERE resolved_at = 0")
        return sorted(rows, key=lambda r: r.get("priority", 0), reverse=True)

    def prune_captain_messages(self, before_timestamp: int) -> bool:
        return self.call("prune_captain_messages", before_timestamp)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pytest apps/heiwa_hub/tests/test_agent_memory.py -v`
Expected: All 6 tests PASS

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_sdk/heiwa_sdk/spacetimedb.py apps/heiwa_hub/tests/test_agent_memory.py
git commit -m "feat(sdk): add STDB bridge methods for captain memory tables"
```

---

### Task 3: AgentMemory Service

**Files:**

- Create: `packages/heiwa_sdk/heiwa_sdk/agent_memory.py`
- Test: `apps/heiwa_hub/tests/test_agent_memory.py` (append to existing)

- [ ] **Step 1: Write failing tests for AgentMemory**

Append to `apps/heiwa_hub/tests/test_agent_memory.py`:

```python
import uuid
import time


class TestAgentMemory:
    """Test the AgentMemory high-level service."""

    def _make_memory(self):
        from heiwa_sdk.agent_memory import AgentMemory
        mem = AgentMemory.__new__(AgentMemory)
        mem.stdb = MagicMock()
        mem.session_id = "test-session"
        mem._token_budget = 8000
        return mem

    def test_store_message(self):
        mem = self._make_memory()
        mem.stdb.insert_captain_message = MagicMock(return_value=True)

        result = mem.store_message(role="operator", content="deploy now", source="discord_dm")
        assert result is True
        mem.stdb.insert_captain_message.assert_called_once()
        call_kw = mem.stdb.insert_captain_message.call_args
        assert call_kw[1]["role"] == "operator"
        assert call_kw[1]["content"] == "deploy now"

    def test_load_context_window(self):
        mem = self._make_memory()
        mem.stdb.get_uncompressed_messages = MagicMock(return_value=[
            {"message_id": "m1", "role": "operator", "content": "hi", "timestamp": 100},
            {"message_id": "m2", "role": "agent", "content": "hello", "timestamp": 200},
        ])
        mem.stdb.get_active_focuses = MagicMock(return_value=[])
        mem.stdb.get_recent_summaries = MagicMock(return_value=[])

        ctx = mem.load_context_window()
        assert len(ctx["messages"]) == 2
        assert ctx["messages"][0]["role"] == "operator"

    def test_needs_compression_under_budget(self):
        mem = self._make_memory()
        # ~10 chars = ~2 tokens, well under 8000
        messages = [{"content": "short msg"}]
        assert mem.needs_compression(messages) is False

    def test_needs_compression_over_budget(self):
        mem = self._make_memory()
        # Each message ~128K chars = ~32K tokens, over 8K budget
        messages = [{"content": "x" * 128000}]
        assert mem.needs_compression(messages) is True

    def test_estimate_tokens(self):
        from heiwa_sdk.agent_memory import AgentMemory
        assert AgentMemory.estimate_tokens("hello world") == 2  # 11 chars // 4 = 2
        assert AgentMemory.estimate_tokens("x" * 100) == 25  # 100 // 4

    def test_detect_complexity_simple(self):
        mem = self._make_memory()
        assert mem.detect_complexity("how's the deploy going?") is False

    def test_detect_complexity_architecture(self):
        mem = self._make_memory()
        assert mem.detect_complexity(
            "I want to refactor the entire deployment pipeline and redesign the strategy for multi-node orchestration"
        ) is True
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pytest apps/heiwa_hub/tests/test_agent_memory.py::TestAgentMemory -v`
Expected: FAIL — `agent_memory` module doesn't exist

- [ ] **Step 3: Implement AgentMemory service**

Create `packages/heiwa_sdk/heiwa_sdk/agent_memory.py`:

```python
"""Heiwa Agent Memory: persistent conversation memory backed by SpacetimeDB."""
from __future__ import annotations

import logging
import time
import uuid
from typing import Any

from heiwa_sdk.spacetimedb import SpacetimeDB

logger = logging.getLogger("SDK.AgentMemory")

# Complexity detection keywords
_COMPLEX_KEYWORDS = frozenset({
    "design", "refactor", "deploy", "strategy", "architecture",
    "migrate", "redesign", "overhaul", "integrate", "think hard",
})

_CHAR_LENGTH_COMPLEX = 200


class AgentMemory:
    """High-level memory service for Heiwa Agent conversations.

    Wraps STDB captain_messages/captain_summaries/captain_focus tables
    with token budgeting, compression detection, and context assembly.
    """

    def __init__(self, stdb: SpacetimeDB, session_id: str | None = None, token_budget: int = 8000):
        self.stdb = stdb
        self.session_id = session_id or str(uuid.uuid4())
        self._token_budget = token_budget

    @staticmethod
    def estimate_tokens(text: str) -> int:
        """Approximate token count: len(text) // 4."""
        return len(text) // 4

    def store_message(self, role: str, content: str, source: str = "discord_dm") -> bool:
        """Store a raw message in captain_messages."""
        return self.stdb.insert_captain_message(
            message_id=str(uuid.uuid4()),
            session_id=self.session_id,
            role=role,
            content=content,
            timestamp=int(time.time() * 1000),
            source=source,
        )

    def load_context_window(self) -> dict[str, Any]:
        """Build the active context window for LLM input."""
        messages = self.stdb.get_uncompressed_messages(limit=100)
        focuses = self.stdb.get_active_focuses()
        summaries = self.stdb.get_recent_summaries(limit=3)
        return {
            "messages": messages,
            "focuses": focuses,
            "summaries": summaries,
        }

    def needs_compression(self, messages: list[dict[str, Any]]) -> bool:
        """Check if uncompressed messages exceed the token budget (~32K chars = ~8K tokens)."""
        total_chars = sum(len(m.get("content", "")) for m in messages)
        return (total_chars // 4) > self._token_budget

    def detect_complexity(self, text: str) -> bool:
        """Detect if a message warrants model escalation."""
        if len(text) > _CHAR_LENGTH_COMPLEX:
            return True
        text_lower = text.lower()
        matches = sum(1 for kw in _COMPLEX_KEYWORDS if kw in text_lower)
        if matches >= 2:
            return True
        question_count = text.count("?")
        if question_count >= 3:
            return True
        return False

    def store_summary(
        self,
        summary_type: str,
        content: str,
        range_start: int,
        range_end: int,
        messages_compressed: int,
    ) -> bool:
        """Store a compression summary."""
        return self.stdb.insert_captain_summary(
            summary_id=str(uuid.uuid4()),
            summary_type=summary_type,
            content=content,
            range_start=range_start,
            range_end=range_end,
            messages_compressed=messages_compressed,
        )

    def mark_compressed(self, before_timestamp: int) -> bool:
        """Mark messages as compressed after summary is stored."""
        return self.stdb.mark_messages_compressed(
            session_id=self.session_id,
            before_timestamp=before_timestamp,
        )

    def upsert_focus(self, topic: str, context: dict[str, Any], priority: int = 3, focus_id: str | None = None) -> str:
        """Create or update a focus tracking entry."""
        import json
        focus_id = focus_id or str(uuid.uuid4())
        self.stdb.upsert_captain_focus(
            focus_id=focus_id,
            topic=topic,
            context_json=json.dumps(context),
            priority=priority,
        )
        return focus_id

    def resolve_focus(self, focus_id: str) -> bool:
        """Mark a focus entry as resolved."""
        return self.stdb.resolve_captain_focus(
            focus_id=focus_id,
            resolved_at=int(time.time() * 1000),
        )
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pytest apps/heiwa_hub/tests/test_agent_memory.py -v`
Expected: All 13 tests PASS (6 bridge + 7 service)

- [ ] **Step 5: Commit**

```bash
git add packages/heiwa_sdk/heiwa_sdk/agent_memory.py apps/heiwa_hub/tests/test_agent_memory.py
git commit -m "feat(sdk): add AgentMemory service for persistent conversation memory"
```

---

## Chunk 2: Heiwa Agent Runtime

### Task 4: Rename Captain → Heiwa Agent

**Files:**

- Create: `apps/heiwa_hub/agents/heiwa_agent.py` (copy from `captain.py`, rename internals)
- Modify: `apps/heiwa_hub/main.py:25,86,96`
- Modify: `apps/heiwa_hub/tests/test_phase3_integration.py:5,24,38`
- Modify: `packages/heiwa_protocol/heiwa_protocol/protocol.py:32`
- Delete: `apps/heiwa_hub/agents/captain.py` (after new file is verified)

- [ ] **Step 1: Create `heiwa_agent.py` from `captain.py`**

Copy `captain.py` to `heiwa_agent.py` and apply these renames:

- Class: `CaptainAgent` → `HeiwaAgent`
- Constructor: `name="heiwa-captain"` → `name="heiwa-agent"`
- Logger: `logging.getLogger("Captain")` → `logging.getLogger("HeiwaAgent")`
- All `logger` messages referencing "Captain" → "Heiwa Agent"

```bash
cp apps/heiwa_hub/agents/captain.py apps/heiwa_hub/agents/heiwa_agent.py
```

Then edit `heiwa_agent.py`:

- Line 2: `"""Heiwa Agent — Machine-Perspective Collaborator`
- Line 29: `logger = logging.getLogger("HeiwaAgent")`
- Line 38: `class HeiwaAgent(BaseAgent):`
- Line 39: docstring: `"""Always-on orchestrator...`
- Line 42: `super().__init__(name="heiwa-agent")`
- Line 74: `logger.info("Heiwa Agent online. ...`

- [ ] **Step 2: Add `HEIWA_AGENT_DM` subject to protocol**

In `packages/heiwa_protocol/heiwa_protocol/protocol.py`, after the `CAPTAIN_DM` line, add:

```python
HEIWA_AGENT_DM = "heiwa.agent.dm"              # Heiwa Agent -> Operator DM
```

Keep `CAPTAIN_DM` in the enum (other code like `CAPTAIN_DIRECTIVE` still uses the captain prefix). Update `heiwa_agent.py` `_dm()` method to use `Subject.HEIWA_AGENT_DM` instead of `Subject.CAPTAIN_DM`.

- [ ] **Step 3: Update main.py import**

In `apps/heiwa_hub/main.py`:

- Line 25: `from heiwa_hub.agents.heiwa_agent import HeiwaAgent`
- Line 86: `captain = HeiwaAgent()`

- [ ] **Step 4: Update test import**

In `apps/heiwa_hub/tests/test_phase3_integration.py`:

- Line 5: `from heiwa_hub.agents.heiwa_agent import HeiwaAgent`
- Lines 24, 38: `agent = HeiwaAgent()`

- [ ] **Step 5: Update Messenger to listen on new subject**

In `apps/heiwa_hub/agents/messenger.py`, update the `CAPTAIN_DM` listener to also listen on `HEIWA_AGENT_DM`:

```python
await self.listen(Subject.HEIWA_AGENT_DM, self.handle_captain_dm)
```

- [ ] **Step 6: Run all tests**

Run: `pytest -v`
Expected: All tests PASS

- [ ] **Step 7: Delete old captain.py**

```bash
git rm apps/heiwa_hub/agents/captain.py
```

- [ ] **Step 8: Commit**

```bash
git add apps/heiwa_hub/agents/heiwa_agent.py apps/heiwa_hub/main.py \
  apps/heiwa_hub/tests/test_phase3_integration.py \
  packages/heiwa_protocol/heiwa_protocol/protocol.py \
  apps/heiwa_hub/agents/messenger.py
git commit -m "refactor: rename CaptainAgent → HeiwaAgent, add HEIWA_AGENT_DM subject"
```

---

### Task 5: Add Memory Loop to Heiwa Agent

This task implements the full 7-step memory cycle from the spec:
RECEIVE → STORE → LOAD → RETRIEVE → REASON → RESPOND → FOCUS

**Files:**

- Modify: `apps/heiwa_hub/agents/heiwa_agent.py`
- Create: `apps/heiwa_hub/tests/test_heiwa_agent.py`

- [ ] **Step 1: Write failing tests for memory integration**

Create `apps/heiwa_hub/tests/test_heiwa_agent.py`:

```python
"""Tests for HeiwaAgent memory loop and compression."""
import pytest
from unittest.mock import MagicMock, AsyncMock, patch


class TestHeiwaAgentMemory:
    """Test memory loop integration in HeiwaAgent."""

    def _make_agent(self):
        with patch("heiwa_hub.agents.heiwa_agent.RepoAuditor"):
            from heiwa_hub.agents.heiwa_agent import HeiwaAgent
            agent = HeiwaAgent()
            agent.db = MagicMock()
            agent.db.stdb = MagicMock()
            agent._llm = MagicMock()
            return agent

    def test_agent_has_memory(self):
        agent = self._make_agent()
        assert agent.agent_memory is not None

    # ── STORE (steps 2 & 6): operator and agent messages persisted ──

    def test_store_operator_message(self):
        agent = self._make_agent()
        agent.agent_memory.store_message = MagicMock(return_value=True)

        agent._store_operator_message("hello from operator", source="discord_dm")

        agent.agent_memory.store_message.assert_called_once_with(
            role="operator", content="hello from operator", source="discord_dm"
        )

    def test_store_agent_response(self):
        agent = self._make_agent()
        agent.agent_memory.store_message = MagicMock(return_value=True)

        agent._store_agent_response("I see the deploy is running")

        agent.agent_memory.store_message.assert_called_once_with(
            role="agent", content="I see the deploy is running", source="system"
        )

    # ── RECEIVE (step 1): event handlers store operator input ──

    @pytest.mark.asyncio
    async def test_on_task_ingress_stores_operator_message(self):
        agent = self._make_agent()
        agent._store_operator_message = MagicMock()
        agent.speak = AsyncMock()

        await agent._on_task_ingress({
            "data": {
                "task_id": "t1",
                "raw_text": "deploy the app",
                "intent_class": "build",
                "source": "discord",
            }
        })

        agent._store_operator_message.assert_called_once()
        call_args = agent._store_operator_message.call_args
        assert "deploy the app" in call_args[0][0]

    # ── COMPRESSION ──

    @pytest.mark.asyncio
    async def test_compression_triggered_when_over_budget(self):
        agent = self._make_agent()
        agent.agent_memory.load_context_window = MagicMock(return_value={
            "messages": [{"content": "x" * 128000, "timestamp": 100}],
            "focuses": [],
            "summaries": [],
        })
        agent.agent_memory.needs_compression = MagicMock(return_value=True)
        agent._run_rolling_compression = AsyncMock()

        await agent._maybe_compress()

        agent._run_rolling_compression.assert_called_once()

    @pytest.mark.asyncio
    async def test_no_compression_when_under_budget(self):
        agent = self._make_agent()
        agent.agent_memory.load_context_window = MagicMock(return_value={
            "messages": [{"content": "short"}],
            "focuses": [],
            "summaries": [],
        })
        agent.agent_memory.needs_compression = MagicMock(return_value=False)
        agent._run_rolling_compression = AsyncMock()

        await agent._maybe_compress()

        agent._run_rolling_compression.assert_not_called()

    # ── BOOT HYDRATION ──

    def test_boot_hydration_loads_context(self):
        agent = self._make_agent()
        agent.agent_memory.load_context_window = MagicMock(return_value={
            "messages": [
                {"role": "operator", "content": "last thing we discussed"},
            ],
            "focuses": [{"topic": "Railway deploy"}],
            "summaries": [{"content": "Yesterday we worked on..."}],
        })

        ctx = agent._hydrate_boot_context()
        assert len(ctx["messages"]) == 1
        assert len(ctx["focuses"]) == 1
        assert len(ctx["summaries"]) == 1

    # ── REASON (step 5): complexity detection for cascade ──

    def test_complexity_detection_routes_to_cascade(self):
        agent = self._make_agent()
        agent.agent_memory.detect_complexity = MagicMock(return_value=True)
        assert agent._should_cascade("redesign the architecture and deploy strategy") is True

    def test_simple_message_stays_on_flash(self):
        agent = self._make_agent()
        agent.agent_memory.detect_complexity = MagicMock(return_value=False)
        assert agent._should_cascade("status?") is False

    # ── FOCUS (step 7): topic tracking ──

    def test_update_focus_creates_entry(self):
        agent = self._make_agent()
        agent.agent_memory.upsert_focus = MagicMock(return_value="focus-123")
        agent.agent_memory.get_active_focuses = MagicMock(return_value=[])

        agent._update_focus("Railway deployment", {"task_id": "t1"})

        agent.agent_memory.upsert_focus.assert_called_once()
        call_kw = agent.agent_memory.upsert_focus.call_args
        assert call_kw[1]["topic"] == "Railway deployment"

    def test_update_focus_updates_existing(self):
        agent = self._make_agent()
        agent.agent_memory.upsert_focus = MagicMock(return_value="focus-existing")
        agent.agent_memory.get_active_focuses = MagicMock(return_value=[
            {"focus_id": "focus-existing", "topic": "Railway deployment", "priority": 3},
        ])

        agent._update_focus("Railway deployment", {"task_id": "t2"})

        agent.agent_memory.upsert_focus.assert_called_once()
        # Should reuse existing focus_id
        call_kw = agent.agent_memory.upsert_focus.call_args
        assert call_kw[1]["focus_id"] == "focus-existing"
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pytest apps/heiwa_hub/tests/test_heiwa_agent.py -v`
Expected: FAIL — `agent_memory` attribute doesn't exist on HeiwaAgent

- [ ] **Step 3: Add memory loop to HeiwaAgent**

In `apps/heiwa_hub/agents/heiwa_agent.py`, add these changes:

**Imports** (add at top):

```python
from heiwa_sdk.agent_memory import AgentMemory
```

**Constructor** (add after `self._errors_seen = 0`):

```python
self.agent_memory = AgentMemory(stdb=self.db.stdb) if self.db.stdb else None
self._boot_context: dict | None = None
```

**Rename `_captain_tick` → `_agent_tick`** (and update the call in `run()`):

```python
# In run():
while self.running:
    await self._agent_tick()
    await asyncio.sleep(CAPTAIN_TICK_SEC)
```

**Wire boot hydration into `run()`** (after the 15s sleep, before the boot DM):

```python
        # Boot hydration: load last session's memory
        self._boot_context = self._hydrate_boot_context()
        boot_ctx_summary = ""
        if self._boot_context.get("summaries"):
            boot_ctx_summary = (
                "\n\nFrom last session:\n> "
                + self._boot_context["summaries"][0].get("content", "")[:200]
            )

        await self._dm(self._build_boot_message() + boot_ctx_summary)
```

**Wire STORE into event handlers** — update `_on_task_ingress` to store:

```python
    async def _on_task_ingress(self, data: dict[str, Any]):
        payload = data.get("data", data)
        task_id = payload.get("task_id", "?")
        raw = str(payload.get("raw_text", ""))[:120]
        intent = payload.get("intent_class", "unknown")
        source = payload.get("source", "unknown")
        self._tasks_seen += 1

        # STORE: persist the incoming task as an operator message
        self._store_operator_message(
            f"[task:{task_id}] {raw}", source=source
        )

        await self._dm(
            f"New task landed from **{source}**.\n"
            f"`{task_id}` classified as **{intent}**\n"
            f"> {raw}\n"
            f"Routing it now — I'll tell you what happens."
        )

        # FOCUS: track active topic
        self._update_focus(intent, {"task_id": task_id, "raw": raw[:80]})
```

**New methods** (add after `_on_error`):

```python
    # ── Memory Loop Methods ──────────────────────────────────

    def _store_operator_message(self, content: str, source: str = "discord_dm"):
        """STORE step: persist operator input to captain_messages."""
        if self.agent_memory:
            self.agent_memory.store_message(role="operator", content=content, source=source)

    def _store_agent_response(self, content: str):
        """STORE step: persist agent output to captain_messages."""
        if self.agent_memory:
            self.agent_memory.store_message(role="agent", content=content, source="system")

    async def _maybe_compress(self):
        """Check if rolling compression is needed and run it."""
        if not self.agent_memory:
            return
        ctx = self.agent_memory.load_context_window()
        messages = ctx.get("messages", [])
        if self.agent_memory.needs_compression(messages):
            await self._run_rolling_compression(messages)

    async def _run_rolling_compression(self, messages: list[dict]):
        """Compress oldest uncompressed messages into a rolling summary."""
        if not messages:
            return
        mid = len(messages) // 2
        to_compress = messages[:mid]
        if not to_compress:
            return

        text_block = "\n".join(
            f"[{m.get('role', '?')}] {m.get('content', '')}" for m in to_compress
        )
        try:
            summary = await self._llm_summarize(text_block)
        except Exception as e:
            logger.error("Rolling compression failed: %s", e)
            return

        range_start = to_compress[0].get("timestamp", 0)
        range_end = to_compress[-1].get("timestamp", 0)

        self.agent_memory.store_summary(
            summary_type="rolling",
            content=summary,
            range_start=range_start,
            range_end=range_end,
            messages_compressed=len(to_compress),
        )
        self.agent_memory.mark_compressed(before_timestamp=range_end + 1)
        logger.info("Rolling compression: %d messages → summary", len(to_compress))

    async def _llm_summarize(self, text: str) -> str:
        """REASON helper: use LLM to generate a summary."""
        prompt = (
            "Summarize this conversation concisely, preserving key decisions, "
            "action items, and technical details:\n\n" + text
        )
        return self.llm.generate(prompt)

    def _hydrate_boot_context(self) -> dict:
        """BOOT: load last session's uncompressed messages + recent summaries."""
        if not self.agent_memory:
            return {"messages": [], "focuses": [], "summaries": []}
        return self.agent_memory.load_context_window()

    def _should_cascade(self, text: str) -> bool:
        """REASON: detect if message needs stronger model (cascade) vs Flash."""
        if not self.agent_memory:
            return False
        return self.agent_memory.detect_complexity(text)

    def _update_focus(self, topic: str, context: dict[str, Any], priority: int = 3):
        """FOCUS step: create or update active focus tracking entry."""
        if not self.agent_memory:
            return
        # Check if there's already an active focus on this topic
        active = self.agent_memory.stdb.get_active_focuses()
        existing = next((f for f in active if f.get("topic") == topic), None)
        if existing:
            self.agent_memory.upsert_focus(
                topic=topic,
                context=context,
                priority=priority,
                focus_id=existing["focus_id"],
            )
        else:
            self.agent_memory.upsert_focus(
                topic=topic,
                context=context,
                priority=priority,
            )
```

**Update `_dm` method** to also store agent responses:

```python
async def _dm(self, content: str):
    """Send DM to operator and persist in memory."""
    self._store_agent_response(content)
    await self.speak(Subject.HEIWA_AGENT_DM, {
        "agent": "heiwa-agent",
        "content": content,
    })
```

**Update `_agent_tick`** (renamed from `_captain_tick`) to call compression:

```python
    async def _agent_tick(self):
        now = time.time()
        system_state = await self._gather_system_state()

        # Memory compression check
        await self._maybe_compress()

        # Handle queued alerts
        alerts = list(self._pending_alerts)
        self._pending_alerts.clear()
        if alerts:
            await self._handle_alerts(alerts, system_state)

        # ... rest of tick unchanged (audit, tuning, maintenance, status)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pytest apps/heiwa_hub/tests/test_heiwa_agent.py -v`
Expected: All 12 tests PASS

- [ ] **Step 5: Run full test suite**

Run: `pytest -v`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_hub/agents/heiwa_agent.py apps/heiwa_hub/tests/test_heiwa_agent.py
git commit -m "feat(agent): add full 7-step memory loop, boot hydration, focus tracking to HeiwaAgent"
```

---

## Chunk 3: Config + Health Check Hardening

### Task 6: Split Antigravity Rate Group

**Files:**

- Modify: `config/swarm/ai_router.json`
- Modify: `config/seeds/model_tiers.json`

- [ ] **Step 1: Split rate group in ai_router.json**

In `config/swarm/ai_router.json`, replace the `google_antigravity` rate limit entry:

Old (line 226-230):

```json
"google_antigravity": {
  "max_turns": 35,
  "window_sec": 3600,
  "cooldown_sec": 60
},
```

New:

```json
"antigravity_flash": {
  "max_turns": 20,
  "window_sec": 3600,
  "cooldown_sec": 60
},
"antigravity_pro": {
  "max_turns": 15,
  "window_sec": 3600,
  "cooldown_sec": 60
},
```

Also update the `google-antigravity` provider block (line 150-156) `rate_group` to `"antigravity_pro"` (the cascade default). **Note:** The per-model `rate_group` in `model_tiers.json` takes precedence over the provider-level default — the routing code resolves rate groups from the model tier entry, not the provider block. The provider-level value is a fallback for models not in the tier matrix.

Also update the `fallbacks` array (line 21): replace `"antigravity/gemini-3-auto"` with `"google-antigravity/gemini-3.1-pro"` (fixes pre-existing wrong prefix + retired model).

- [ ] **Step 2: Split model tiers in model_tiers.json**

Replace the single `antigravity/gemini-3-auto` entry with two entries:

```json
{
  "model_id": "google-antigravity/gemini-3-flash",
  "provider_model_id": "gemini-3-flash",
  "provider": "google-antigravity",
  "rate_group": "antigravity_flash",
  "capability_class": 2,
  "effort_knob": "thinking:on",
  "effort_level": 3,
  "cost_per_turn": 0.0,
  "max_context_tokens": 1000000,
  "strengths": ["chat", "status", "observation"],
  "enabled": true
},
{
  "model_id": "google-antigravity/gemini-3.1-pro",
  "provider_model_id": "gemini-3.1-pro",
  "provider": "google-antigravity",
  "rate_group": "antigravity_pro",
  "capability_class": 3,
  "effort_knob": "thinking:always",
  "effort_level": 5,
  "cost_per_turn": 0.0,
  "max_context_tokens": 1000000,
  "strengths": ["strategy", "review", "research", "architecture"],
  "enabled": true
},
```

- [ ] **Step 3: Update model registry in ai_router.json**

Replace `class_3_strategy` entry:

Old:

```json
"class_3_strategy": {
  "id": "google-antigravity/gemini-3-auto",
  "provider": "google-antigravity",
  ...
}
```

New — split into two entries:

```json
"heiwa_agent_routine": {
  "id": "google-antigravity/gemini-3-flash",
  "provider": "google-antigravity",
  "host_node": "railway@heiwa-cloud-hq",
  "compute_class": 2,
  "role": "heiwa_agent_routine_reasoning"
},
"class_3_strategy": {
  "id": "google-antigravity/gemini-3.1-pro",
  "provider": "google-antigravity",
  "host_node": "railway@heiwa-cloud-hq",
  "compute_class": 3,
  "role": "strategy_adversarial_review"
}
```

Also update `provider_rotation` references — replace `google-antigravity/gemini-3-auto` with `google-antigravity/gemini-3.1-pro` in all four locations:

- `premium_remote` array (line 278)
- `by_intent.research` array (line 285)
- `by_intent.strategy` array (line 291)
- `by_intent.build` array (line 301)

- [ ] **Step 4: Commit**

```bash
git add config/swarm/ai_router.json config/seeds/model_tiers.json
git commit -m "feat(config): split antigravity rate group into flash (20/hr) and pro (15/hr)"
```

---

### Task 7: Harden Health Check

**Files:**

- Modify: `apps/heiwa_hub/mcp_server.py:216-225`
- Test: `apps/heiwa_hub/tests/test_heiwa_agent.py` (append)

- [ ] **Step 1: Write failing test for health check**

Append to `apps/heiwa_hub/tests/test_heiwa_agent.py`:

```python
class TestHealthCheck:
    """Test STDB-aware health check."""

    @pytest.mark.asyncio
    async def test_health_returns_503_when_stdb_down(self):
        from unittest.mock import PropertyMock
        with patch("apps.heiwa_hub.mcp_server.db") as mock_db:
            mock_db.state_backend = "spacetimedb"
            mock_db.stdb = MagicMock()
            mock_db.stdb.query = MagicMock(return_value=[])
            # Simulate timeout
            mock_db.stdb.query.side_effect = Exception("connection refused")

            from apps.heiwa_hub.mcp_server import _check_stdb_health
            result = await _check_stdb_health()
            assert result is False

    @pytest.mark.asyncio
    async def test_health_returns_200_when_stdb_up(self):
        with patch("apps.heiwa_hub.mcp_server.db") as mock_db:
            mock_db.state_backend = "spacetimedb"
            mock_db.stdb = MagicMock()
            mock_db.stdb.query = MagicMock(return_value=[{"1": 1}])

            from apps.heiwa_hub.mcp_server import _check_stdb_health
            result = await _check_stdb_health()
            assert result is True
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pytest apps/heiwa_hub/tests/test_heiwa_agent.py::TestHealthCheck -v`
Expected: FAIL — `_check_stdb_health` doesn't exist

- [ ] **Step 3: Implement STDB-aware health check**

In `apps/heiwa_hub/mcp_server.py`, replace the health endpoint:

```python
import asyncio

async def _check_stdb_health() -> bool:
    """Verify STDB connectivity with 2-second timeout."""
    if db.state_backend != "spacetimedb" or not db.stdb:
        return True  # Non-STDB backends are always "healthy"
    try:
        result = await asyncio.wait_for(
            asyncio.to_thread(db.stdb.query, "SELECT 1 FROM model_tiers LIMIT 1"),
            timeout=2.0,
        )
        return result is not None
    except Exception:
        return False


@app.get("/health")
@app.head("/health")
async def health():
    stdb_ok = await _check_stdb_health()
    if not stdb_ok:
        from starlette.responses import JSONResponse
        return JSONResponse(
            status_code=503,
            content={
                "status": "degraded",
                "service": "heiwa-core-hub",
                "stdb": "unreachable",
                "timestamp": time.time(),
            },
        )
    return {
        "status": "alive",
        "service": "heiwa-core-hub",
        "state_backend": db.state_backend,
        "stdb": "connected",
        "gateway_transport": "websocket",
        "timestamp": time.time(),
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `pytest apps/heiwa_hub/tests/test_heiwa_agent.py::TestHealthCheck -v`
Expected: PASS

- [ ] **Step 5: Run full test suite**

Run: `pytest -v`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_hub/mcp_server.py apps/heiwa_hub/tests/test_heiwa_agent.py
git commit -m "feat(health): STDB-aware health check with 2s timeout, returns 503 when unreachable"
```

---

### Task 8: STDB Data Directory for Railway

**Files:**

- Modify: `apps/heiwa_hub/start.sh:125-131`

- [ ] **Step 1: Add STDB data directory config**

In `apps/heiwa_hub/start.sh`, before the SpacetimeDB pre-flight check (line 122), add:

```bash
# Configure STDB persistent data directory (Railway volume)
STDB_DATA_DIR="${STDB_DATA_DIR:-}"
if [[ -n "$STDB_DATA_DIR" ]] && [[ ! -d "$STDB_DATA_DIR" ]]; then
    echo "[HEIWA] Creating STDB data directory at $STDB_DATA_DIR..."
    mkdir -p "$STDB_DATA_DIR"
fi
```

Update the `spacetime start` line to use the data directory:

```bash
if [[ -n "$STDB_DATA_DIR" ]]; then
    echo "[HEIWA] Starting local SpacetimeDB with persistent volume at $STDB_DATA_DIR..."
    spacetime start --listen-addr 127.0.0.1:3000 --data-dir "$STDB_DATA_DIR" &
else
    spacetime start --listen-addr 127.0.0.1:3000 &
fi
```

- [ ] **Step 2: Commit**

```bash
git add apps/heiwa_hub/start.sh
git commit -m "feat(deploy): support STDB_DATA_DIR for Railway persistent volume"
```

---

### Task 9: Final Integration Test

- [ ] **Step 1: Run full test suite**

Run: `pytest -v`
Expected: All tests PASS

- [ ] **Step 2: Verify no import errors**

Run: `python -c "from heiwa_hub.agents.heiwa_agent import HeiwaAgent; print('OK')"`
Expected: `OK`

Run: `python -c "from heiwa_sdk.agent_memory import AgentMemory; print('OK')"`
Expected: `OK`

- [ ] **Step 3: Verify Rust builds**

Run: `cd apps/heiwa_hub/spacetimedb && spacetime build 2>&1 | tail -3`
Expected: Build success

- [ ] **Step 4: Final commit (if any uncommitted changes)**

```bash
git status
# If clean, no action needed
```
