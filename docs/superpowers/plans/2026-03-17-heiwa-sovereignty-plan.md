# Sovereignty Foundation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a secure, fast, and SpacetimeDB-native foundation for the Heiwa mesh loop by centralizing security, purging legacy SQLite, and persisting telemetry.

**Architecture:** 
1. **SecurityService**: A centralized class in `heiwa_sdk/security.py` that handles `HEIWA_AUTH_TOKEN` validation, replacing manual `os.getenv` checks across the Hub.
2. **SpacetimeDB Sovereignty**: Remove all `sqlite3` fallback logic from `heiwa_sdk/db.py` to silence migration warnings and enforce STDB as the single source of truth.
3. **Telemetry Persistence**: Migrate `TelemetryAgent`'s ephemeral usage cache to STDB via `db.py` so the mesh retains rate-limit awareness across restarts.

**Tech Stack:** Python, SpacetimeDB CLI/SDK, Pytest.

---

## Chunk 1: Centralized Security Service

**Files:**
- Modify: `packages/heiwa_sdk/heiwa_sdk/security.py`
- Modify: `apps/heiwa_hub/mcp_server.py`
- Modify: `apps/heiwa_hub/agents/spine.py`
- Modify: `apps/heiwa_hub/transport.py`
- Test: `apps/heiwa_hub/tests/test_security_posture.py`

### Task 1: Create SecurityService

- [ ] **Step 1: Write failing test for SecurityService**
Update `test_security_posture.py` to import `SecurityService` and assert `validate_token` works with valid/invalid tokens.
- [ ] **Step 2: Run test to verify it fails**
Run: `export PYTHONPATH=$(pwd)/packages/heiwa_sdk:$(pwd)/apps && pytest apps/heiwa_hub/tests/test_security_posture.py -k "SecurityService"`
Expected: FAIL (ImportError)
- [ ] **Step 3: Implement SecurityService**
In `security.py`, add the `SecurityService` class with `get_expected_token()` and `validate_token(token)` methods. It should read from `heiwa_sdk.config.settings.HEIWA_AUTH_TOKEN`.
- [ ] **Step 4: Run test to verify it passes**
Run: `export PYTHONPATH=$(pwd)/packages/heiwa_sdk:$(pwd)/apps && pytest apps/heiwa_hub/tests/test_security_posture.py -k "SecurityService"`
Expected: PASS
- [ ] **Step 5: Commit**
```bash
git add packages/heiwa_sdk/heiwa_sdk/security.py apps/heiwa_hub/tests/test_security_posture.py
git commit -m "feat(sdk): add SecurityService for centralized auth token validation"
```

### Task 2: Refactor Hub to use SecurityService

- [ ] **Step 1: Refactor mcp_server.py**
Replace the `_validate_auth_token` function body with `SecurityService().validate_token(token)`. Raise HTTPException if false.
- [ ] **Step 2: Refactor spine.py**
In `handle_request`, replace the `expected_token` manual check with `SecurityService().validate_token(auth_token)`.
- [ ] **Step 3: Refactor transport.py**
In `WorkerSessionManager.worker_socket`, replace `expected_token = os.getenv("HEIWA_AUTH_TOKEN", "")` and the subsequent check with `SecurityService().validate_token(token)`.
- [ ] **Step 4: Run Hub tests**
Run: `export PYTHONPATH=$(pwd)/packages/heiwa_sdk:$(pwd)/apps && pytest apps/heiwa_hub/tests/`
Expected: PASS (Existing tests should verify the barrier).
- [ ] **Step 5: Commit**
```bash
git add apps/heiwa_hub/
git commit -m "refactor(hub): centralize digital barrier validation via SecurityService"
```

## Chunk 2: SQLite Purge

**Files:**
- Modify: `packages/heiwa_sdk/heiwa_sdk/db.py`
- Test: `apps/heiwa_hub/tests/test_state_service.py`

### Task 3: Remove SQLite Fallbacks

- [ ] **Step 1: Write failing test**
Update `test_state_service.py` to ensure `Database` raises an error or warns if initialized with `spacetimedb` but STDB is unavailable, instead of falling back to SQLite silently.
- [ ] **Step 2: Run test to verify it fails**
Run: `export PYTHONPATH=$(pwd)/packages/heiwa_sdk:$(pwd)/apps && pytest apps/heiwa_hub/tests/test_state_service.py`
Expected: FAIL
- [ ] **Step 3: Strip SQLite from db.py**
Remove `import sqlite3`. Delete `_init_sqlite_schema`, `get_connection`, `_exec`. Update `init_db` to only check for STDB. Update CRUD methods to return early or fail if `self.stdb` is missing.
- [ ] **Step 4: Run test to verify it passes**
Run: `export PYTHONPATH=$(pwd)/packages/heiwa_sdk:$(pwd)/apps && pytest apps/heiwa_hub/tests/test_state_service.py`
Expected: PASS
- [ ] **Step 5: Commit**
```bash
git add packages/heiwa_sdk/heiwa_sdk/db.py apps/heiwa_hub/tests/test_state_service.py
git commit -m "refactor(sdk): purge legacy sqlite fallback to enforce stdb sovereignty"
```

## Chunk 3: Telemetry Migration

**Files:**
- Modify: `apps/heiwa_hub/agents/telemetry.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- Test: `apps/heiwa_hub/tests/test_memory_service.py`

### Task 4: Move Usage Cache to STDB

- [ ] **Step 1: Write STDB ledger method test**
Update tests to verify `spacetimedb.py` has a method to aggregate usage from the `runs` table (e.g. `get_model_usage_summary` already exists, test it).
- [ ] **Step 2: Refactor TelemetryAgent**
Remove `self.usage_cache` dict. In `handle_status_query`, call `self.db.get_model_usage_summary(minutes=60)` directly to return real-time aggregated STDB stats instead of using an ephemeral cache.
- [ ] **Step 3: Run integration tests**
Run: `export PYTHONPATH=$(pwd)/packages/heiwa_sdk:$(pwd)/apps && pytest apps/heiwa_hub/tests/ -k "telemetry or state"`
Expected: PASS
- [ ] **Step 4: Commit**
```bash
git add apps/heiwa_hub/agents/telemetry.py packages/heiwa_sdk/heiwa_sdk/spacetimedb.py
git commit -m "feat(hub): migrate telemetry usage cache to spacetimedb ledger"
```