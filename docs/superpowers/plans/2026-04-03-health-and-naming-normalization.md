# Health and Naming Normalization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Normalize health check endpoints (`/health`) and service naming across the monorepo.

**Architecture:** Consolidate `/health` and `/ready` into a single `/health` endpoint that returns 200 when ready and 503 otherwise. Unify service naming to `heiwa-core`, `heiwa-hub`, and `heiwa-trading`.

**Tech Stack:** Rust (Axum), Python (FastAPI/Starlette), Railway.

---

### Task 1: Normalize heiwa-core (Rust)

**Files:**

- Modify: `apps/heiwa_core/src/runtime/mod.rs`
- Modify: `apps/heiwa_core/src/config.rs`
- Modify: `apps/heiwa_core/src/runtime/state.rs`

- [ ] **Step 1: Update `SystemStatus` to include `as_str` method**

Modify `apps/heiwa_core/src/runtime/state.rs`:

```rust
impl SystemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SystemStatus::Starting => "starting",
            SystemStatus::Ready => "ok",
            SystemStatus::Degraded => "degraded",
        }
    }
}
```

- [ ] **Step 2: Update `health_handler` and `ready_handler` in `apps/heiwa_core/src/runtime/mod.rs`**

```rust
async fn health_handler(axum::extract::State(state): axum::extract::State<SharedState>) -> impl IntoResponse {
    let status = state.status.read().await;
    let is_ready = *status == SystemStatus::Ready;
    
    let body = Json(json!({
        "status": status.as_str(),
        "service": "heiwa-core",
        "ready": is_ready,
        "timestamp": time::OffsetDateTime::now_utc().unix_timestamp(),
    }));

    if is_ready {
        (axum::http::StatusCode::OK, body)
    } else {
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, body)
    }
}
```

- [ ] **Step 3: Remove `/ready` route and update `build_router` in `apps/heiwa_core/src/runtime/mod.rs`**

- [ ] **Step 4: Update `heartbeat` function and `config.rs` defaults**
      Change `"cloud-hq"` to `"heiwa-core"`.

- [ ] **Step 5: Run `cargo test -p heiwa-core` and verify**

- [ ] **Step 6: Commit**
      `git add apps/heiwa_core && git commit -m "feat(core): normalize health endpoint and naming"`

### Task 2: Normalize heiwa-hub (Python)

**Files:**

- Modify: `apps/heiwa_hub/mcp_server.py`

- [ ] **Step 1: Update `/health` endpoint**

```python
@app.get("/health")
@app.head("/health")
async def health():
    stdb_ok = await _check_stdb_health()
    status = "ok" if stdb_ok else "degraded"
    
    payload = {
        "status": status,
        "service": "heiwa-hub",
        "ready": stdb_ok,
        "state_backend": db.state_backend,
        "stdb": "connected" if stdb_ok else "unreachable",
        "timestamp": time.time(),
    }
    
    if not stdb_ok:
        from starlette.responses import JSONResponse
        return JSONResponse(status_code=503, content=payload)
    return payload
```

- [ ] **Step 2: Update tests for heiwa-hub**
      Update `apps/heiwa_hub/tests/test_phase5_integration.py` and others that check for `"alive"`.

- [ ] **Step 3: Commit**
      `git add apps/heiwa_hub && git commit -m "feat(hub): normalize health endpoint and naming"`

### Task 3: Normalize heiwa-trading and heiwa-sdk (Python)

**Files:**

- Modify: `apps/heiwa_trading/src/heiwa_trading/app.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/main.py`

- [ ] **Step 1: Update `/health` in `heiwa_trading`**

- [ ] **Step 2: Update `/health` in `heiwa_sdk`**

- [ ] **Step 3: Run `pytest apps/heiwa_trading` and `pytest packages/heiwa_sdk`**

- [ ] **Step 4: Commit**
      `git add apps/heiwa_trading packages/heiwa_sdk && git commit -m "feat(trading,sdk): normalize health endpoints"`

### Task 4: Infrastructure Alignment

**Files:**

- Modify: `railway.toml`
- Modify: `apps/heiwa_trading/railway.toml`
- Modify: `apps/heiwa_web/clients/web/assets/domains.bootstrap.json`

- [ ] **Step 1: Update `railway.toml` to use `healthcheckPath = "/health"`**

- [ ] **Step 2: Update `apps/heiwa_trading/railway.toml` to use `healthcheckPath = "/health"` (if not already)**

- [ ] **Step 3: Update `domains.bootstrap.json` service names**

- [ ] **Step 4: Verify overall system status with a mock check**

- [ ] **Step 5: Commit**
      `git add . && git commit -m "chore(infra): align health checks and service names"`
