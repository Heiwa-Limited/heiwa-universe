# Heiwa Terminal v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the Heiwa shell from an offline dev tool into a real evidence-emitting BYOK terminal client that registers devices, records receipts, and syncs provider status to SpacetimeDB.

**Architecture:** The `heiwa_shell` binary connects to SpacetimeDB using the same `DbConnection::builder()` pattern that `heiwa_core::runtime` already uses. A new `crates/heiwa_stdb/` crate owns the connection lifecycle and provides a typed evidence API. The shell, REPL, and loop crate all emit evidence through this crate. Everything degrades gracefully to offline mode when STDB is unreachable.

**Tech Stack:** Rust, SpacetimeDB SDK 2.0.3, heiwa-bindings (auto-generated), macOS Keychain (`security` CLI), tokio async runtime

---

## File Structure

### New files

| File                                          | Responsibility                                                                  |
| --------------------------------------------- | ------------------------------------------------------------------------------- |
| `crates/heiwa_stdb/Cargo.toml`                | New crate: STDB connection lifecycle + evidence API for the local shell         |
| `crates/heiwa_stdb/src/lib.rs`                | `StdbClient` struct: connect, heartbeat, device registration, evidence emission |
| `crates/heiwa_stdb/src/evidence.rs`           | Typed helpers for recording route decisions, runs, failures, provider status    |
| `crates/heiwa_stdb/tests/offline_fallback.rs` | Tests that all evidence methods degrade gracefully when disconnected            |
| `apps/heiwa_shell/tests/stdb_connection.rs`   | Integration test: shell boots with and without STDB env vars                    |

### Modified files

| File                               | What changes                                                                                                                                            |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Cargo.toml` (workspace root)      | Add `crates/heiwa_stdb` to workspace members                                                                                                            |
| `apps/heiwa_shell/Cargo.toml`      | Add `heiwa-stdb` dependency                                                                                                                             |
| `apps/heiwa_shell/src/main.rs`     | Replace `attempt_stdb_connection()` with real `StdbClient`; wire device registration, heartbeat, REPL evidence emission; remove hardcoded HOME fallback |
| `crates/heiwa_provider/src/lib.rs` | Remove hardcoded `/Users/dmcgregsauce` HOME fallback                                                                                                    |
| `crates/heiwa_install/src/lib.rs`  | Remove hardcoded `/Users/dmcgregsauce` HOME fallback                                                                                                    |
| `crates/heiwa_session/src/lib.rs`  | Remove hardcoded `/Users/dmcgregsauce` HOME fallback                                                                                                    |
| `crates/heiwa_loop/Cargo.toml`     | Add `heiwa-stdb` dependency                                                                                                                             |
| `crates/heiwa_loop/src/lib.rs`     | Accept `StdbClient` instead of raw `Option<Arc<DbConnection>>`                                                                                          |

---

## Task 1: Remove hardcoded HOME fallbacks

**Files:**

- Modify: `crates/heiwa_provider/src/lib.rs:34`
- Modify: `crates/heiwa_install/src/lib.rs:33`
- Modify: `crates/heiwa_session/src/lib.rs:17`

These three files all have `"/Users/dmcgregsauce"` as a HOME env var fallback. Replace with a panic that surfaces the problem clearly instead of silently using a wrong path on another user's machine.

- [ ] **Step 1: Fix `heiwa_provider`**

In `crates/heiwa_provider/src/lib.rs`, replace line 34:

```rust
fn get_heiwa_state_dir() -> PathBuf {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .expect("HOME or USERPROFILE must be set");
    PathBuf::from(home).join(".heiwa")
}
```

- [ ] **Step 2: Fix `heiwa_install`**

In `crates/heiwa_install/src/lib.rs`, replace line 33:

```rust
pub fn get_heiwa_dir() -> PathBuf {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .expect("HOME or USERPROFILE must be set");
    PathBuf::from(home).join(".heiwa")
}
```

- [ ] **Step 3: Fix `heiwa_session`**

In `crates/heiwa_session/src/lib.rs`, replace line 17:

```rust
pub fn get_session_dir() -> PathBuf {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .expect("HOME or USERPROFILE must be set");
    PathBuf::from(home).join(".heiwa").join("sessions")
}
```

- [ ] **Step 4: Run existing tests to verify nothing broke**

Run: `cargo test -p heiwa-provider -- --nocapture`
Run: `cargo test -p heiwa-install -- --nocapture`
Expected: all pass (HOME is always set in dev/CI environments)

- [ ] **Step 5: Commit**

```bash
git add crates/heiwa_provider/src/lib.rs crates/heiwa_install/src/lib.rs crates/heiwa_session/src/lib.rs
git commit -m "fix: remove hardcoded /Users/dmcgregsauce HOME fallback from all crates"
```

---

## Task 2: Create `heiwa_stdb` crate — connection lifecycle

This crate owns the STDB connection for the local shell. It reads the same env vars as `heiwa_core::config.rs` (`STDB_URL`, `STDB_IDENTITY`, `STDB_TOKEN`) so configuration is consistent between the hosted runtime and the local shell.

**Files:**

- Create: `crates/heiwa_stdb/Cargo.toml`
- Create: `crates/heiwa_stdb/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create `crates/heiwa_stdb/Cargo.toml`**

```toml
[package]
name = "heiwa-stdb"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1.0"
heiwa-bindings = { path = "../../packages/heiwa_bindings/rust" }
tokio = { version = "1.50.0", features = ["sync", "time"] }
tracing = "0.1"
```

- [ ] **Step 2: Create `crates/heiwa_stdb/src/lib.rs`**

```rust
pub mod evidence;

use std::env;
use std::sync::Arc;
use anyhow::{anyhow, Result};
use heiwa_bindings::DbConnection;
use tokio::sync::watch;

/// Configuration for connecting to SpacetimeDB from the local shell.
///
/// Reads the same env vars as `heiwa_core::config::RuntimeConfig`:
/// - `STDB_URL` (default: `https://maincloud.spacetimedb.com`)
/// - `STDB_IDENTITY` / `STDB_DATABASE` (default: `heiwaproductiondb`)
/// - `STDB_TOKEN` / `STDB_AUTH_TOKEN` / `SPACETIMEDB_TOKEN` (default: empty)
#[derive(Debug, Clone)]
pub struct StdbConfig {
    pub url: String,
    pub database: String,
    pub token: String,
}

impl StdbConfig {
    pub fn from_env() -> Option<Self> {
        let url = env::var("STDB_URL").unwrap_or_else(|_| {
            let server = env::var("STDB_SERVER").unwrap_or_else(|_| "maincloud".to_string());
            if server == "local" {
                "http://localhost:3000".to_string()
            } else {
                "https://maincloud.spacetimedb.com".to_string()
            }
        });

        let database = env::var("STDB_IDENTITY")
            .or_else(|_| env::var("STDB_DATABASE"))
            .unwrap_or_else(|_| "heiwaproductiondb".to_string());

        let token = env::var("STDB_TOKEN")
            .or_else(|_| env::var("STDB_AUTH_TOKEN"))
            .or_else(|_| env::var("SPACETIMEDB_TOKEN"))
            .unwrap_or_default();

        Some(Self { url, database, token })
    }
}

/// STDB client for the local `heiwa` shell.
///
/// Wraps `Option<Arc<DbConnection>>` so callers never need to handle
/// the connected/disconnected split — evidence methods are no-ops
/// when disconnected.
#[derive(Clone)]
pub struct StdbClient {
    conn: Option<Arc<DbConnection>>,
    connected_tx: watch::Sender<bool>,
    connected_rx: watch::Receiver<bool>,
}

impl StdbClient {
    /// Create a disconnected client. Use `connect()` to establish a connection.
    pub fn offline() -> Self {
        let (tx, rx) = watch::channel(false);
        Self { conn: None, connected_tx: tx, connected_rx: rx }
    }

    /// Attempt to connect to STDB. Returns Ok even if connection fails —
    /// the client degrades to offline mode.
    pub async fn connect(config: &StdbConfig) -> Self {
        let (tx, rx) = watch::channel(false);

        let conn = match DbConnection::builder()
            .with_uri(&config.url)
            .with_database_name(&config.database)
            .with_token(if config.token.is_empty() { None } else { Some(&config.token) })
            .build()
        {
            Ok(conn) => {
                tracing::info!("Connected to SpacetimeDB at {}/{}", config.url, config.database);
                let _ = tx.send(true);
                Some(Arc::new(conn))
            }
            Err(e) => {
                tracing::warn!("STDB connection failed (offline mode): {}", e);
                None
            }
        };

        Self { conn, connected_tx: tx, connected_rx: rx }
    }

    /// Get the raw connection, if connected.
    pub fn connection(&self) -> Option<&Arc<DbConnection>> {
        self.conn.as_ref()
    }

    /// Whether the client has a live connection.
    pub fn is_connected(&self) -> bool {
        self.conn.is_some()
    }

    /// Subscribe to connection state changes.
    pub fn connected_rx(&self) -> watch::Receiver<bool> {
        self.connected_rx.clone()
    }

    /// Start the background message advancement loop.
    /// Must be called after connect() for the connection to process messages.
    pub fn spawn_advance_loop(&self) {
        if let Some(conn) = self.conn.clone() {
            tokio::spawn(async move {
                loop {
                    if let Err(e) = conn.advance_one_message_async().await {
                        tracing::warn!("STDB advance error: {:?}", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
            });
        }
    }
}
```

- [ ] **Step 3: Add to workspace**

In the root `Cargo.toml`, add `"crates/heiwa_stdb"` to the workspace members list, after `"crates/heiwa_loop"`.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p heiwa-stdb`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add crates/heiwa_stdb/ Cargo.toml
git commit -m "feat: add heiwa_stdb crate — STDB connection lifecycle for local shell"
```

---

## Task 3: Add evidence emission helpers to `heiwa_stdb`

**Files:**

- Create: `crates/heiwa_stdb/src/evidence.rs`

These are typed helpers that call STDB reducers. Every method is a no-op when disconnected, so callers never need to check connection state.

- [ ] **Step 1: Write the offline fallback test**

Create `crates/heiwa_stdb/tests/offline_fallback.rs`:

```rust
use heiwa_stdb::StdbClient;

#[tokio::test]
async fn evidence_methods_are_noops_when_offline() {
    let client = StdbClient::offline();
    assert!(!client.is_connected());

    // All evidence methods should return Ok when offline
    assert!(client.register_device("dev-1", "user-1", "test-host", "macos", "aarch64").is_ok());
    assert!(client.heartbeat_device("dev-1").is_ok());
    assert!(client.sync_provider_status("acct-1", "anthropic", "dev-1", "api_key", "local-ref", "connected", None, None, "[]").is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p heiwa-stdb --test offline_fallback -- --nocapture`
Expected: FAIL — methods don't exist yet

- [ ] **Step 3: Write `evidence.rs`**

Create `crates/heiwa_stdb/src/evidence.rs`:

```rust
use crate::StdbClient;
use anyhow::Result;

impl StdbClient {
    /// Register or update a device in STDB.
    pub fn register_device(
        &self,
        device_id: &str,
        user_id: &str,
        hostname: &str,
        os: &str,
        arch: &str,
    ) -> Result<()> {
        let conn = match self.connection() {
            Some(c) => c,
            None => return Ok(()),
        };
        conn.reducers
            .register_device(
                device_id.to_string(),
                user_id.to_string(),
                hostname.to_string(),
                os.to_string(),
                arch.to_string(),
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }

    /// Send a device heartbeat.
    pub fn heartbeat_device(&self, device_id: &str) -> Result<()> {
        let conn = match self.connection() {
            Some(c) => c,
            None => return Ok(()),
        };
        conn.reducers
            .update_device_heartbeat(device_id.to_string())
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }

    /// Sync a provider account's status to STDB.
    pub fn sync_provider_status(
        &self,
        account_id: &str,
        provider_id: &str,
        node_id: &str,
        auth_kind: &str,
        local_handle_ref: &str,
        status: &str,
        display_name: Option<String>,
        default_model: Option<String>,
        available_models_json: &str,
    ) -> Result<()> {
        let conn = match self.connection() {
            Some(c) => c,
            None => return Ok(()),
        };
        conn.reducers
            .upsert_provider_account_status(
                account_id.to_string(),
                provider_id.to_string(),
                node_id.to_string(),
                auth_kind.to_string(),
                local_handle_ref.to_string(),
                status.to_string(),
                display_name,
                default_model,
                "local".to_string(), // rate_group
                available_models_json.to_string(),
                None, // user_id — set by caller if multi-tenant
                None, // owner_id
                None, // principal_id
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }

    /// Record a DREX route decision.
    pub fn record_route_decision(
        &self,
        request_id: &str,
        task_id: &str,
        raw_text: &str,
        intent_class: &str,
        risk_level: &str,
        privacy_level: &str,
        assigned_worker: &str,
        target_tool: &str,
        target_model: &str,
        target_runtime: &str,
        rationale: &str,
        confidence: f64,
    ) -> Result<()> {
        let conn = match self.connection() {
            Some(c) => c,
            None => return Ok(()),
        };
        conn.reducers
            .record_route_decision(
                request_id.to_string(),
                task_id.to_string(),
                "v1".to_string(), // envelope_version
                raw_text.to_string(),
                "terminal".to_string(), // source_surface
                intent_class.to_string(),
                risk_level.to_string(),
                privacy_level.to_string(),
                0, // compute_class
                assigned_worker.to_string(),
                target_tool.to_string(),
                target_model.to_string(),
                target_runtime.to_string(),
                "auto".to_string(), // target_tier
                false, // requires_approval
                rationale.to_string(),
                confidence,
                "local".to_string(), // gateway_transport
                None, // user_id
                None, // owner_id
                None, // principal_id
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }

    /// Record a completed or failed run.
    pub fn record_run(
        &self,
        run_id: &str,
        user_id: &str,
        proposal_id: &str,
        started_at: &str,
        ended_at: &str,
        status: &str,
        model_id: &str,
        tokens_input: i64,
        tokens_output: i64,
        cost: f64,
        session_id: Option<String>,
        failure_code: Option<String>,
        failure_message: Option<String>,
    ) -> Result<()> {
        let conn = match self.connection() {
            Some(c) => c,
            None => return Ok(()),
        };
        conn.reducers
            .record_run(
                run_id.to_string(),
                user_id.to_string(),
                proposal_id.to_string(),
                "local-lease".to_string(), // lease_id
                session_id,
                started_at.to_string(),
                ended_at.to_string(),
                status.to_string(),
                "{}".to_string(), // chain_result_json
                "{}".to_string(), // signals_json
                "[]".to_string(), // artifact_index_json
                "local".to_string(), // node_id
                "{}".to_string(), // replay_receipt_json
                "terminal".to_string(), // mode
                model_id.to_string(),
                tokens_input,
                tokens_output,
                tokens_input + tokens_output,
                cost,
                None, // owner_id
                None, // principal_id
                failure_code,
                failure_message,
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p heiwa-stdb --test offline_fallback -- --nocapture`
Expected: PASS — all methods return Ok(()) when offline

- [ ] **Step 5: Commit**

```bash
git add crates/heiwa_stdb/src/evidence.rs crates/heiwa_stdb/tests/offline_fallback.rs
git commit -m "feat: add evidence emission helpers to heiwa_stdb with offline fallback"
```

---

## Task 4: Wire STDB connection into `heiwa_shell`

Replace `attempt_stdb_connection()` in `main.rs` with real `StdbClient`. Add `heiwa-stdb` dependency.

**Files:**

- Modify: `apps/heiwa_shell/Cargo.toml`
- Modify: `apps/heiwa_shell/src/main.rs`

- [ ] **Step 1: Add dependency**

In `apps/heiwa_shell/Cargo.toml`, add to `[dependencies]`:

```toml
heiwa-stdb = { path = "../../crates/heiwa_stdb" }
uuid = { version = "1.0", features = ["v4"] }
```

- [ ] **Step 2: Replace `attempt_stdb_connection()`**

In `apps/heiwa_shell/src/main.rs`, replace the function at line 443-447:

```rust
async fn attempt_stdb_connection() -> heiwa_stdb::StdbClient {
    match heiwa_stdb::StdbConfig::from_env() {
        Some(config) => {
            let client = heiwa_stdb::StdbClient::connect(&config).await;
            client.spawn_advance_loop();
            client
        }
        None => heiwa_stdb::StdbClient::offline(),
    }
}
```

- [ ] **Step 3: Update call sites**

The function's return type changed from `Option<Arc<DbConnection>>` to `StdbClient`. Update all call sites in `main.rs`:

1. In the `"loop"` command handler (~line 295), change:
   ```rust
   let stdb = attempt_stdb_connection().await;
   ```
   The variable `stdb` is passed to `LoopController::new()` — this will be updated in Task 6 when we refactor the loop crate.
   For now, bridge it:
   ```rust
   let stdb_client = attempt_stdb_connection().await;
   let stdb = stdb_client.connection().cloned();
   ```

2. In `run_repl()` (~line 449), add STDB client initialization at the start:
   ```rust
   let stdb_client = attempt_stdb_connection().await;
   if stdb_client.is_connected() {
       println!("  Connected to SpacetimeDB");
   } else {
       println!("  Running in offline mode (set STDB_URL to enable sync)");
   }
   ```

- [ ] **Step 4: Write integration test**

Create `apps/heiwa_shell/tests/stdb_connection.rs`:

```rust
use std::process::Command;

#[test]
fn shell_boots_without_stdb_env_vars() {
    let output = Command::new("cargo")
        .args(&["run", "-p", "heiwa-shell", "--bin", "heiwa", "--", "doctor"])
        .env_remove("STDB_URL")
        .env_remove("STDB_TOKEN")
        .output()
        .expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Heiwa Doctor Report"));
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p heiwa-shell --test stdb_connection -- --nocapture`
Run: `cargo test -p heiwa-shell --test smoke -- --nocapture`
Expected: both pass

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_shell/Cargo.toml apps/heiwa_shell/src/main.rs apps/heiwa_shell/tests/stdb_connection.rs
git commit -m "feat: wire real STDB connection into heiwa_shell via StdbClient"
```

---

## Task 5: Wire device registration and heartbeat

Make `register_current_device()` actually call the `register_device` reducer, and start a heartbeat loop.

**Files:**

- Modify: `apps/heiwa_shell/src/main.rs`

- [ ] **Step 1: Rewrite `register_current_device()`**

Replace the current `register_current_device()` function (lines 374-405) with:

```rust
async fn register_current_device(stdb_client: &heiwa_stdb::StdbClient) -> Result<()> {
    let identity = match heiwa_provider::load_identity() {
        Some(id) => id,
        None => {
            println!("Not logged in. Please run 'heiwa login' first.");
            return Ok(());
        }
    };

    let _report = heiwa_install::check_installation()?;
    let manifest_path = heiwa_install::get_heiwa_dir().join("machine.json");

    let device_id = if manifest_path.exists() {
        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;
        manifest["device_id"].as_str().unwrap_or("unknown").to_string()
    } else {
        "unknown".to_string()
    };

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    println!("Registering device {} for user {}...", device_id, identity.user_id);

    stdb_client.register_device(
        &device_id,
        &identity.user_id,
        &hostname,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )?;

    // Sync provider statuses
    let mut registry = heiwa_provider::AccountRegistry::load();
    heiwa_provider::detect::auto_discover(&mut registry).await;
    for account in &registry.accounts {
        let models_json = serde_json::to_string(&account.models).unwrap_or_else(|_| "[]".to_string());
        stdb_client.sync_provider_status(
            &account.account_id,
            &account.provider,
            &device_id,
            account.credential.kind_label(),
            &account.account_id, // local_handle_ref
            &format!("{:?}", account.status),
            None,
            None,
            &models_json,
        )?;
        println!("  Synced provider {} status: {:?}", account.provider, account.status);
    }

    if stdb_client.is_connected() {
        println!("Device and capabilities synced to SpacetimeDB.");
    } else {
        println!("Device registered locally (STDB offline — will sync when connected).");
    }
    Ok(())
}
```

- [ ] **Step 2: Add `hostname` dependency**

In `apps/heiwa_shell/Cargo.toml`, add:

```toml
hostname = "0.4"
```

- [ ] **Step 3: Update call sites**

In the `"install"` command handler (~line 36), pass the stdb client:

```rust
"install" => {
    heiwa_install::run_install()?;
    println!("Registering device...");
    let stdb_client = attempt_stdb_connection().await;
    register_current_device(&stdb_client).await?;
}
```

In the `"register"` command handler (~line 51):

```rust
"register" => {
    let stdb_client = attempt_stdb_connection().await;
    register_current_device(&stdb_client).await?;
}
```

- [ ] **Step 4: Add heartbeat to REPL**

In `run_repl()`, after the STDB client initialization, spawn a heartbeat task:

```rust
// Start device heartbeat if connected
let heartbeat_device_id = {
    let manifest_path = heiwa_install::get_heiwa_dir().join("machine.json");
    if manifest_path.exists() {
        std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|m| m["device_id"].as_str().map(|s| s.to_string()))
    } else {
        None
    }
};

if let Some(ref dev_id) = heartbeat_device_id {
    let stdb_hb = stdb_client.clone();
    let dev_id_clone = dev_id.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let _ = stdb_hb.heartbeat_device(&dev_id_clone);
        }
    });
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p heiwa-shell --test smoke -- --nocapture`
Expected: all pass

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_shell/Cargo.toml apps/heiwa_shell/src/main.rs
git commit -m "feat: wire real device registration and heartbeat to STDB"
```

---

## Task 6: Wire REPL evidence emission

Every task execution in the REPL should record a route decision and a run receipt.

**Files:**

- Modify: `apps/heiwa_shell/src/main.rs`

- [ ] **Step 1: Add evidence emission after DREX routing in the REPL**

In the `ReplCommand::Task(t)` handler, after `plan_route()` succeeds and before adapter execution (~line 562), record the route decision:

```rust
let request_id = uuid::Uuid::new_v4().to_string();
let turn_started_at = chrono::Utc::now().to_rfc3339();

let _ = stdb_client.record_route_decision(
    &request_id,
    &request_id, // task_id = request_id for REPL tasks
    &t,
    &infer_task_intent(&t),
    "low",
    "standard",
    &selected.provider,
    &selected.provider,
    &selected.model_id,
    if is_local_provider_check(&selected.provider) { "local" } else { "remote" },
    &route.routing_metadata,
    0.9,
);
```

Where `is_local_provider_check` is a small helper (add near the other helpers at the bottom of main.rs):

```rust
fn is_local_provider_check(provider: &str) -> bool {
    matches!(provider, "ollama" | "local" | "vllm" | "litellm")
}
```

- [ ] **Step 2: Add evidence emission after adapter completes**

After the usage printout (~line 628), record the run:

```rust
let turn_ended_at = chrono::Utc::now().to_rfc3339();
let user_id = heiwa_provider::load_identity()
    .map(|id| id.user_id)
    .unwrap_or_else(|| "anonymous".to_string());

if let Some(ref u) = usage {
    let _ = stdb_client.record_run(
        &format!("run-{}", uuid::Uuid::new_v4()),
        &user_id,
        &request_id,
        &turn_started_at,
        &turn_ended_at,
        "SUCCESS",
        &selected.model_id,
        u.input_tokens as i64,
        u.output_tokens as i64,
        u.cost_usd,
        None,
        None,
        None,
    );
} else {
    let _ = stdb_client.record_run(
        &format!("run-{}", uuid::Uuid::new_v4()),
        &user_id,
        &request_id,
        &turn_started_at,
        &turn_ended_at,
        "COMPLETED_NO_USAGE",
        &selected.model_id,
        0,
        0,
        0.0,
        None,
        None,
        None,
    );
}
```

Note: `selected` needs to be captured before the adapter spawn. It already is — `selected` is bound at line 563 and used at line 572 for adapter resolution. Clone `selected.model_id` and `selected.provider` into local variables before the `tokio::spawn` to avoid borrow issues:

```rust
let selected_model_id = selected.model_id.clone();
let selected_provider = selected.provider.clone();
```

Then use `selected_model_id` and `selected_provider` in the evidence calls.

- [ ] **Step 3: Run tests**

Run: `cargo test -p heiwa-shell --test smoke -- --nocapture`
Expected: all pass (evidence emission is no-op when STDB not configured)

- [ ] **Step 4: Commit**

```bash
git add apps/heiwa_shell/src/main.rs
git commit -m "feat: emit route decision and run receipt evidence from REPL"
```

---

## Task 7: Migrate loop crate to `StdbClient`

The loop crate currently takes `Option<Arc<DbConnection>>`. Migrate it to accept `StdbClient` so it gets the same offline-graceful behavior and evidence API.

**Files:**

- Modify: `crates/heiwa_loop/Cargo.toml`
- Modify: `crates/heiwa_loop/src/lib.rs`
- Modify: `apps/heiwa_shell/src/main.rs` (remove bridge)

- [ ] **Step 1: Add dependency**

In `crates/heiwa_loop/Cargo.toml`, add:

```toml
heiwa-stdb = { path = "../../crates/heiwa_stdb" }
```

- [ ] **Step 2: Update `LoopController`**

In `crates/heiwa_loop/src/lib.rs`, change the `stdb` field type:

```rust
pub struct LoopController {
    config: LoopConfig,
    loop_id: String,
    cancelled: Arc<AtomicBool>,
    stdb: heiwa_stdb::StdbClient,
    model_tiers: Vec<ModelTier>,
}
```

Update `new()`:

```rust
pub fn new(config: LoopConfig, stdb: heiwa_stdb::StdbClient, model_tiers: Vec<ModelTier>) -> Self {
    Self {
        config,
        loop_id: Uuid::new_v4().to_string(),
        cancelled: Arc::new(AtomicBool::new(false)),
        stdb,
        model_tiers,
    }
}
```

In `run()`, replace all `if let Some(ref stdb) = self.stdb { stdb.reducers.* }` blocks with direct calls through the connection:

Replace the loop session start (~line 76-84):

```rust
if let Some(conn) = self.stdb.connection() {
    conn.reducers.start_loop_session(
        self.loop_id.clone(),
        self.config.user_id.clone(),
        self.config.objective.clone(),
        self.config.max_turns,
        self.config.max_cost_usd,
    ).map_err(|e| anyhow!(e.to_string()))?;
}
```

Replace the cancellation handler (~line 93-99):

```rust
if let Some(conn) = self.stdb.connection() {
    conn.reducers.complete_loop_session(
        self.loop_id.clone(),
        "CANCELLED".to_string(),
        "User requested cancellation".to_string(),
    ).map_err(|e| anyhow!(e.to_string()))?;
}
```

Replace the run recording block (~line 179-214):

```rust
if let Some(conn) = self.stdb.connection() {
    conn.reducers.record_run(
        run_id.clone(),
        self.config.user_id.clone(),
        format!("loop-{}", self.loop_id),
        "loop-lease".to_string(),
        Some(self.loop_id.clone()),
        turn_started_at,
        turn_ended_at,
        "SUCCESS".to_string(),
        "{}".to_string(),
        "{}".to_string(),
        "[]".to_string(),
        "local-node".to_string(),
        "{}".to_string(),
        "loop".to_string(),
        selected_tier.model_id.clone(),
        turn_usage.input_tokens as i64,
        turn_usage.output_tokens as i64,
        0,
        turn_cost,
        None, None,
        None, None,
    ).map_err(|e| anyhow!(e.to_string()))?;

    let iteration_id = Uuid::new_v4().to_string();
    conn.reducers.record_loop_iteration(
        iteration_id,
        self.loop_id.clone(),
        current_turn,
        ingress.raw_text.clone(),
        output_summary,
        0.5,
        Some(run_id),
        turn_cost,
    ).map_err(|e| anyhow!(e.to_string()))?;
}
```

Replace the completion block (~line 231-237):

```rust
if let Some(conn) = self.stdb.connection() {
    conn.reducers.complete_loop_session(
        self.loop_id.clone(),
        "COMPLETED".to_string(),
        "Max turns reached or objective met".to_string(),
    ).map_err(|e| anyhow!(e.to_string()))?;
}
```

- [ ] **Step 3: Update shell call site**

In `apps/heiwa_shell/src/main.rs`, in the `"loop"` command handler, remove the bridge and pass `StdbClient` directly:

```rust
let stdb_client = attempt_stdb_connection().await;
let controller = heiwa_loop::LoopController::new(config, stdb_client, model_tiers);
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p heiwa-loop -- --nocapture`
Run: `cargo test -p heiwa-shell --test smoke -- --nocapture`
Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add crates/heiwa_loop/Cargo.toml crates/heiwa_loop/src/lib.rs apps/heiwa_shell/src/main.rs
git commit -m "refactor: migrate loop crate from raw DbConnection to StdbClient"
```

---

## Task 8: Add `heiwa receipts` command

This is the first user-visible proof that evidence is being recorded. Show recent runs from local state (and later from STDB).

**Files:**

- Modify: `apps/heiwa_shell/src/main.rs`

- [ ] **Step 1: Add the command handler**

In `main.rs`, add a new match arm in the CLI command router (after the `"session"` arm, ~line 262):

```rust
"receipts" => {
    let stdb_client = attempt_stdb_connection().await;
    if !stdb_client.is_connected() {
        println!("Not connected to SpacetimeDB. Receipts require a live connection.");
        println!("Set STDB_URL and STDB_TOKEN environment variables to enable.");
    } else {
        println!("Connected to SpacetimeDB — run receipts are being recorded.");
        println!("Query receipts via: spacetime sql heiwaproductiondb \"SELECT * FROM runs ORDER BY ended_at DESC LIMIT 10\"");
    }
}
```

- [ ] **Step 2: Add to help text**

In `print_help()`, add after the `session attach` line:

```rust
println!("  receipts                      Show run receipt status");
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p heiwa-shell --test smoke -- --nocapture`
Expected: all pass

- [ ] **Step 4: Commit**

```bash
git add apps/heiwa_shell/src/main.rs
git commit -m "feat: add 'heiwa receipts' command to show evidence status"
```

---

## Task 9: Add `heiwa devices` command

Show registered devices from the local machine manifest, indicating sync status.

**Files:**

- Modify: `apps/heiwa_shell/src/main.rs`

- [ ] **Step 1: Add the command handler**

In `main.rs`, add a new match arm (after `"receipts"`):

```rust
"devices" => {
    let manifest_path = heiwa_install::get_heiwa_dir().join("machine.json");
    if manifest_path.exists() {
        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;
        println!("Devices:");
        println!("  ID:       {}", manifest["device_id"].as_str().unwrap_or("unknown"));
        println!("  Hostname: {}", manifest["hostname"].as_str().unwrap_or("unknown"));
        println!("  OS:       {}", manifest["os"].as_str().unwrap_or("unknown"));
        println!("  Arch:     {}", manifest["arch"].as_str().unwrap_or("unknown"));
        println!("  Installed: {}", manifest["installed_at"].as_str().unwrap_or("unknown"));

        let stdb_client = attempt_stdb_connection().await;
        if stdb_client.is_connected() {
            println!("  Sync:     Connected to SpacetimeDB");
        } else {
            println!("  Sync:     Offline (local only)");
        }
    } else {
        println!("No device registered. Run 'heiwa install' first.");
    }
}
```

- [ ] **Step 2: Add to help text**

In `print_help()`:

```rust
println!("  devices                       Show registered devices");
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p heiwa-shell --test smoke -- --nocapture`
Expected: all pass

- [ ] **Step 4: Commit**

```bash
git add apps/heiwa_shell/src/main.rs
git commit -m "feat: add 'heiwa devices' command showing local device and sync status"
```

---

## Task 10: Verify full workspace compiles and passes

**Files:** none (verification only)

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace`
Expected: compiles with no errors

- [ ] **Step 2: Full workspace test**

Run: `cargo test -p heiwa-shell --test smoke -- --nocapture`
Run: `cargo test -p heiwa-stdb -- --nocapture`
Run: `cargo test -p heiwa-loop -- --nocapture`
Run: `cargo test -p heiwa-provider -- --nocapture`
Expected: all pass

- [ ] **Step 3: Manual smoke test**

Run: `cargo run -p heiwa-shell --bin heiwa -- doctor`
Expected: shows doctor report without errors

Run: `cargo run -p heiwa-shell --bin heiwa -- devices`
Expected: shows device info or "No device registered"

Run: `cargo run -p heiwa-shell --bin heiwa -- receipts`
Expected: shows offline message (unless STDB env vars are set)

Run: `cargo run -p heiwa-shell --bin heiwa -- help`
Expected: shows all commands including `devices` and `receipts`

- [ ] **Step 4: Commit any remaining fixes**

```bash
git add -A
git commit -m "chore: verify full workspace builds and tests pass after STDB wiring"
```

---

## What this plan does NOT cover (Stage 2+)

These are explicitly deferred and should be separate plans:

1. **Additional provider adapters** (Codex HTTP API, Gemini HTTP API, Antigravity) — requires separate adapter implementations per provider
2. **Real `heiwa login`** — replace hardcoded devon-canonical with Heiwa Hub token validation or OAuth flow
3. **Session attach** — requires bidirectional WebSocket session relay
4. **Cross-platform keychain** — Linux libsecret, Windows Credential Manager fallbacks
5. **`HEIWA.md` sync pass** — rewrite canonical truth doc after code truth is landed
6. **STDB secret model cleanup** — decide on `OAuthIdentity`/`ProviderCredential` table usage for hosted vs local paths
7. **`heiwa receipts` rich query** — subscribe to STDB `runs` table and render locally
8. **Multi-device sync** — heartbeat coordination, session handoff, device mesh
