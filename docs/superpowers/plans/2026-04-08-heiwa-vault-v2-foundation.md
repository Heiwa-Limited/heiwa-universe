# Heiwa Vault V2 Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Normalize Heiwa around live provider truth and build the first canonical foundation for a user-scoped, cross-node, Markdown-first vault that replaces the current thin memory layer.

**Architecture:** Start by fixing the provider/model vocabulary so routing and account state stop lying. Then land STDB-native `vault_*` primitives, add a Rust-first local vault workspace/read-model path, and finally bridge the existing Python memory helpers onto the new vault substrate instead of letting them remain the product center.

**Tech Stack:** Rust, SpacetimeDB 2.0, generated Rust/TypeScript bindings, Python compatibility layer, local Ollama, checked-in JSON config

---

### Task 1: Normalize Live Provider Truth

**Files:**
- Modify: `config/seeds/model_tiers.json`
- Modify: `config/swarm/ai_router.json`
- Modify: `crates/heiwa_provider/src/registry.rs`
- Modify: `crates/heiwa_provider/src/detect/mod.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Test: `apps/heiwa_hub/tests/test_compute_router_stdb.py`
- Create: `crates/heiwa_provider/tests/live_provider_normalization.rs`

- [ ] **Step 1: Write a failing Rust test for provider normalization**

Create `crates/heiwa_provider/tests/live_provider_normalization.rs` with assertions that the canonical provider ids and rate groups are:

- `ollama` → `local_ollama`
- `google-gemini-cli` → `google_gemini_cli`
- `google-antigravity` → `antigravity_flash`, `antigravity_pro`
- `claude-code` → `claude_code`
- `codex` → `openai_codex`

- [ ] **Step 2: Run the new Rust test and confirm failure**

Run: `cargo test -p heiwa-provider live_provider_normalization`

Expected: FAIL because the current registry and discovery logic still use generic provider ids and incomplete CLI inventory handling.

- [ ] **Step 3: Update checked-in routing truth**

Edit `config/seeds/model_tiers.json` and `config/swarm/ai_router.json` so they:

- remove `codex/gpt-4.1` as a primary live coding tier
- remove `gemini-2.5-*` as current canonical truth
- preserve only the live provider set
- keep `google-antigravity` split into `antigravity_flash` and `antigravity_pro`
- prefer `ollama/qwen3.5:9b` for local code and `ollama/gemma4` for local chat

- [ ] **Step 4: Update registry vocabulary and CLI discovery**

Modify `crates/heiwa_provider/src/registry.rs` and `crates/heiwa_provider/src/detect/mod.rs` so the generated account/model inventory uses the repo’s canonical provider ids instead of generic `google`, `openai`, and `anthropic`.

- [ ] **Step 5: Update shell live-tier generation**

Modify `apps/heiwa_shell/src/main.rs` so `get_live_model_tiers` preserves the canonical provider/rate-group vocabulary and stops flattening everything into legacy local/cloud buckets that hide the real provider surface.

- [ ] **Step 6: Add/adjust Python router coverage**

Update `apps/heiwa_hub/tests/test_compute_router_stdb.py` to reflect the normalized live-only provider set and remove stale `gpt-4.1` / legacy Antigravity expectations.

- [ ] **Step 7: Run focused verification**

Run:

```bash
cargo test -p heiwa-provider live_provider_normalization
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_compute_router_stdb.py
```

Expected: PASS

### Task 2: Land Canonical Vault Schema in STDB

**Files:**
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- Test: `apps/heiwa_hub/tests/test_vault_stdb.py`
- Regenerate: `packages/heiwa_bindings/rust/`
- Regenerate: `packages/heiwa_bindings/typescript/`

- [ ] **Step 1: Write failing STDB bridge tests**

Create `apps/heiwa_hub/tests/test_vault_stdb.py` covering:

- `upsert_vault_document`
- `append_vault_chunk`
- `upsert_vault_link`
- `upsert_vault_entity`
- `upsert_vault_fact`
- `upsert_vault_sync_state`

- [ ] **Step 2: Run the new STDB bridge tests and confirm failure**

Run: `pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_vault_stdb.py`

Expected: FAIL because the `vault_*` bridge methods do not exist.

- [ ] **Step 3: Add `vault_*` tables and reducers**

Modify `apps/heiwa_hub/spacetimedb/src/lib.rs` to add:

- `vault_documents`
- `vault_chunks`
- `vault_links`
- `vault_entities`
- `vault_facts`
- `vault_sync_state`

Add reducers for idempotent upsert/append behavior.

- [ ] **Step 4: Extend the Python STDB bridge**

Modify `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` to expose the new reducers and query helpers in the same style as the existing `captain_*` and `model_tiers` bridges.

- [ ] **Step 5: Regenerate bindings**

Run the repo’s STDB binding generation workflow so the Rust and TypeScript bindings reflect the new schema.

- [ ] **Step 6: Re-run focused schema verification**

Run:

```bash
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_vault_stdb.py
```

Expected: PASS

### Task 3: Create a Rust-First Local Vault Workspace and Read Model

**Files:**
- Create: `crates/heiwa_vault/Cargo.toml`
- Create: `crates/heiwa_vault/src/lib.rs`
- Create: `crates/heiwa_vault/src/workspace.rs`
- Create: `crates/heiwa_vault/src/materialize.rs`
- Create: `crates/heiwa_vault/src/index.rs`
- Create: `crates/heiwa_vault/src/search.rs`
- Create: `crates/heiwa_vault/tests/workspace_roundtrip.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Write a failing round-trip test**

Create `crates/heiwa_vault/tests/workspace_roundtrip.rs` that:

- creates a temporary vault root
- writes a Markdown note
- materializes it into canonical document/chunk records
- hydrates a local read model
- confirms the note can be recalled by full-text search

- [ ] **Step 2: Run the new vault test and confirm failure**

Run: `cargo test -p heiwa-vault workspace_roundtrip`

Expected: FAIL because the crate does not exist.

- [ ] **Step 3: Add the new crate and workspace sync primitives**

Create `crates/heiwa_vault` with:

- workspace path resolution under `~/.heiwa/vault/<owner_id>/`
- Markdown note scanning and hash comparison
- canonical document/chunk materialization helpers
- local read-model hydration hooks

- [ ] **Step 4: Add a minimal local search implementation**

Implement a first-pass exact/fuzzy full-text search path in `src/search.rs` so the crate can prove the round-trip end to end before semantic reranking exists.

- [ ] **Step 5: Re-run vault crate verification**

Run: `cargo test -p heiwa-vault workspace_roundtrip`

Expected: PASS

### Task 4: Ingest Conversations and Filesystem From Day One

**Files:**
- Create: `crates/heiwa_vault/src/ingest/conversations.rs`
- Create: `crates/heiwa_vault/src/ingest/filesystem.rs`
- Create: `crates/heiwa_vault/tests/conversation_ingest.rs`
- Create: `crates/heiwa_vault/tests/filesystem_ingest.rs`
- Modify: `crates/heiwa_vault/src/lib.rs`

- [ ] **Step 1: Write a failing conversation-ingest test**

Create `crates/heiwa_vault/tests/conversation_ingest.rs` that:

- ingests a small provider-style transcript
- creates a canonical `vault_document`
- emits ordered `vault_chunks`
- preserves verbatim text

- [ ] **Step 2: Write a failing filesystem-ingest test**

Create `crates/heiwa_vault/tests/filesystem_ingest.rs` that:

- ingests a small Markdown/code tree
- respects ignore rules
- emits documents/chunks with stable source paths

- [ ] **Step 3: Run the ingest tests and confirm failure**

Run:

```bash
cargo test -p heiwa-vault conversation_ingest
cargo test -p heiwa-vault filesystem_ingest
```

Expected: FAIL

- [ ] **Step 4: Implement the two ingest paths**

Add:

- conversation parsing and transcript chunking
- filesystem scanning with ignore behavior
- source lineage metadata
- owner/user/domain/topic assignment

- [ ] **Step 5: Re-run ingest verification**

Run:

```bash
cargo test -p heiwa-vault conversation_ingest
cargo test -p heiwa-vault filesystem_ingest
```

Expected: PASS

### Task 5: Bridge Existing Heiwa Memory Surfaces Onto the Vault

**Files:**
- Modify: `packages/heiwa_sdk/heiwa_sdk/agent_memory.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/memory.py`
- Modify: `apps/heiwa_hub/agents/heiwaclaw.py`
- Modify: `apps/heiwa_hub/tests/test_agent_memory.py`
- Modify: `apps/heiwa_hub/tests/test_memory_service.py`

- [ ] **Step 1: Write failing compatibility tests**

Extend the existing tests so they assert:

- `AgentMemory` reads from vault-backed context rather than only `captain_messages`
- `MemoryService` indexes and queries through the new vault substrate
- `HeiwaClaw` can hydrate boot context from vault records

- [ ] **Step 2: Run the focused Python tests and confirm failure**

Run:

```bash
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_agent_memory.py
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_memory_service.py
```

Expected: FAIL or reveal missing vault integration paths.

- [ ] **Step 3: Add compatibility shims**

Modify the Python memory layer so it becomes a compatibility facade over vault-backed retrieval and indexing rather than remaining the primary product implementation.

- [ ] **Step 4: Update HeiwaClaw boot hydration and recall**

Modify `apps/heiwa_hub/agents/heiwaclaw.py` so boot context and recall prefer vault documents/chunks/materialized notes, with `captain_*` only as migration fallback.

- [ ] **Step 5: Re-run focused Python verification**

Run:

```bash
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_agent_memory.py
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_memory_service.py
```

Expected: PASS

### Task 6: Expose the Vault Through `heiwa`

**Files:**
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `crates/heiwa_loop/src/lib.rs`
- Create: `crates/heiwa_vault/tests/shell_vault_commands.rs`

- [ ] **Step 1: Write a failing shell-level test**

Create `crates/heiwa_vault/tests/shell_vault_commands.rs` or an equivalent shell integration test that asserts `heiwa` can:

- show vault status
- search the vault
- surface the active vault root for the current owner

- [ ] **Step 2: Run the new shell-level test and confirm failure**

Run: `cargo test -p heiwa-vault shell_vault_commands`

Expected: FAIL

- [ ] **Step 3: Add initial vault commands to `heiwa`**

Modify `apps/heiwa_shell/src/main.rs` to add operator-facing commands for:

- vault status
- vault search
- vault sync status

- [ ] **Step 4: Re-run shell verification**

Run: `cargo test -p heiwa-vault shell_vault_commands`

Expected: PASS

### Final Verification

- [ ] **Step 1: Run the full focused verification set**

Run:

```bash
cargo test -p heiwa-provider live_provider_normalization
cargo test -p heiwa-vault
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_vault_stdb.py
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_agent_memory.py
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_memory_service.py
pytest -q /Users/dmcgregsauce/heiwa-universe/apps/heiwa_hub/tests/test_compute_router_stdb.py
```

Expected: PASS

- [ ] **Step 2: Manually verify runtime truth**

Run:

```bash
/Users/dmcgregsauce/.heiwa/bin/heiwa-route status
heiwa
```

Expected:

- live providers reflect the normalized provider/model/rate-group truth
- the shell sees vault-backed memory surfaces instead of only thin session memory
