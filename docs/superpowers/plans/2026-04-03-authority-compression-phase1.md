# Authority Compression Phase 1: Device Semantics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Elevate `Device` (currently `NodeStatus`) to a first-class citizen in SpacetimeDB with robust capability advertising.

**Architecture:** Update the `NodeStatus` table in STDB to include structured fields for VRAM, models, and trust tiers. Update `heiwa-core` and Python workers to heartbeat this rich metadata.

**Tech Stack:** Rust (STDB, Axum), Python (SDK).

---

### Task 1: Update STDB Schema

**Files:**
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`

- [ ] **Step 1: Update `NodeStatus` struct**

```rust
#[table(accessor = nodes, public)]
pub struct NodeStatus {
    #[primary_key]
    pub node_id: String,
    pub last_heartbeat_at: String,
    pub first_seen_at: String,
    #[index(btree)]
    pub last_seen_at: String,
    #[index(btree)]
    pub status: String,
    pub meta_json: String,
    pub capabilities_json: String, // Keep for arbitrary/extensible caps
    pub agent_version: String,
    pub tags_json: String,
    pub max_concurrency: i64,
    
    // New structured fields for DREX
    pub vram_mb: i64,
    pub locality: String,          // "local", "cloud", "mesh"
    pub trust_tier: i32,           // 0 (untrusted) to 10 (sovereign)
    pub provider_keys_json: String, // ["openai", "anthropic", "google"]
    pub model_inventory_json: String, // ["ollama/llama3", "1bit/scout"]
}
```

- [ ] **Step 2: Update `upsert_node_heartbeat` reducer**
Update signature and logic to handle new fields.

- [ ] **Step 3: Run `cargo build` in STDB module**
`cd apps/heiwa_hub/spacetimedb && cargo build`

### Task 2: Update heiwa-core Advertising (Rust)

**Files:**
- Modify: `apps/heiwa_core/src/runtime/mod.rs`
- Modify: `packages/heiwa_bindings/rust/src/lib.rs` (if manually maintained, or regenerate)

- [ ] **Step 1: Update `heartbeat` function in `heiwa-core`**
Update the call to `upsert_node_heartbeat` with real/mocked hardware data (e.g., from env or `sysinfo`).

- [ ] **Step 2: Commit core changes**

### Task 3: Update Python SDK & Workers (Python)

**Files:**
- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- Modify: `packages/heiwa_identity/heiwa_identity/node.py`

- [ ] **Step 1: Update Python binding for `upsert_node_heartbeat`**

- [ ] **Step 2: Update `NodeIdentity` to gather rich hardware info**
Update `load_node_identity()` to detect VRAM (via `nvidia-smi` or similar) and local models.

- [ ] **Step 3: Verify with integration test**
`uv run pytest scripts/tests/test_device_advertising.py` (New test)

---

### Task 4: Integration Verification

- [ ] **Step 1: Start local STDB**
- [ ] **Step 2: Run heiwa-core**
- [ ] **Step 3: Check STDB tables for rich device data**
`spacetime sql "SELECT * FROM nodes"`
