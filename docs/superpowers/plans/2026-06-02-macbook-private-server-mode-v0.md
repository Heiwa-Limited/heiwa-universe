# MacBook Private Server Mode v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn Devon's MacBook into a private, localhost-only Heiwa server that prefers local models under resource budgets, keeps provider agents always-on through Heiwa-managed coordination, and exposes truthful state in Heiwa.app.

**Architecture:** The installed `heiwa` runtime remains the authority under `~/.heiwa` and serves only `127.0.0.1:7474`. Local models run first when machine pressure allows; provider CLIs are escalation lanes. Cross-provider agents communicate by writing A2A-shaped messages, status, and artifacts into local Heiwa state, not by calling each other directly.

**Tech Stack:** Rust workspace (`apps/heiwa_shell`, `crates/heiwa_provider`, `crates/heiwa_a2a`, new `crates/heiwa_resource`, new `crates/heiwa_agent_bus`), launchd, Ollama, provider CLIs, local JSON/JSONL state under `~/.heiwa/state`, existing cockpit TypeScript.

---

## Scope Check

This plan is deliberately local-only. It does not ship public GitHub/product surfaces, expose ports, add Cloudflare, add Browserbase cloud automation, or sell Heiwa Limited. Browserbase CLI and Odysseus are reference patterns only for v0. The runnable target is Devon's private app on `127.0.0.1`.

Current blockers this plan addresses:

- `ltd.heiwa.app` launchd PATH cannot see `/Users/dmcgregsauce/.local/bin/claude`, `/Users/dmcgregsauce/.npm-global/bin/codex`, or `/Users/dmcgregsauce/.npm-global/bin/gemini`.
- Local model use is preferred but not yet governed by CPU, memory, battery, thermal, and active-worker budgets.
- Worker heartbeats exist, and `crates/heiwa_a2a` has I/O-free envelopes, but there is no local agent bus for provider agents to coordinate.
- Heiwa.app lacks one private-server status view that shows provider parity, resource policy, agent coordination, and approval state.

## File Structure

Create:

- `crates/heiwa_resource/Cargo.toml` — shared crate for local resource snapshots and admission policy.
- `crates/heiwa_resource/src/lib.rs` — public API for `ResourceSnapshot`, `ResourcePolicy`, and `Admission`.
- `crates/heiwa_resource/tests/policy.rs` — pure policy tests.
- `crates/heiwa_agent_bus/Cargo.toml` — shared crate for local A2A-shaped coordination state.
- `crates/heiwa_agent_bus/src/lib.rs` — `AgentBus`, `AgentRecord`, task/status append/list/claim APIs.
- `crates/heiwa_agent_bus/tests/spool.rs` — local tempdir tests for bus persistence.
- `apps/heiwa_shell/src/cmd/agents.rs` — CLI for local agent registry, inbox, send, status.
- `docs/local-private-server.md` — operator contract for private server mode.

Modify:

- `Cargo.toml` — add the two new crates as workspace members.
- `crates/heiwa_provider/src/lib.rs` — replace `which`-only command detection with a deterministic provider command resolver.
- `crates/heiwa_provider/src/providers/{claude_code,codex_cli,gemini_cli}.rs` — spawn resolved provider binaries, not bare command names.
- `apps/heiwa_shell/src/cmd/mod.rs` and `apps/heiwa_shell/src/cli.rs` — wire `heiwa agents`.
- `apps/heiwa_shell/src/main.rs` — expose `agents` help and integrate resource admission into route preview / REPL routing.
- `apps/heiwa_shell/src/cmd/app.rs` — add `/api/v1/private-server`, `/api/v1/resource`, and `/api/v1/agents` payloads.
- `apps/heiwa_shell/tests/smoke.rs` — add command/API smoke coverage.
- `apps/heiwa_app/clients/cockpit/src/lib/types.ts` — add private server/resource/agent types.
- `apps/heiwa_app/clients/cockpit/src/lib/endpoints.ts` — add endpoints.
- `apps/heiwa_app/clients/cockpit/src/routes/Today.tsx` or a new local status component — show private server state.
- `docs/local-self-operation.md` — link private server mode and resource policy.

Do not touch:

- Public install pages except docs links if explicitly required.
- Cloudflare, Railway, public domains, or GitHub release logic.
- Durable `~/.heiwa/state` contents except through normal runtime/test commands.

---

### Task 1: Provider Command Resolution Parity

**Files:**
- Modify: `crates/heiwa_provider/src/lib.rs:315`
- Modify: `crates/heiwa_provider/src/providers/claude_code.rs:35`
- Modify: `crates/heiwa_provider/src/providers/codex_cli.rs:35`
- Modify: `crates/heiwa_provider/src/providers/gemini_cli.rs:35`
- Test: `crates/heiwa_provider/src/lib.rs`

- [ ] **Step 1: Add failing tests for launchd-like PATH**

Add tests near the provider auth tests in `crates/heiwa_provider/src/lib.rs`:

```rust
#[cfg(test)]
mod command_resolution_tests {
    use super::*;

    #[test]
    fn provider_search_paths_include_user_local_bins() {
        let home = PathBuf::from("/Users/devon");
        let paths = provider_search_paths_for_home(&home);
        assert!(paths.contains(&home.join(".local").join("bin")));
        assert!(paths.contains(&home.join(".npm-global").join("bin")));
        assert!(paths.contains(&home.join(".heiwa").join("bin")));
        assert!(paths.contains(&home.join(".cargo").join("bin")));
    }

    #[test]
    fn resolve_command_prefers_path_then_known_user_bins() {
        let temp = std::env::temp_dir().join(format!(
            "heiwa-provider-test-{}",
            std::process::id()
        ));
        let bin = temp.join(".npm-global").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("codex");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();

        let resolved = resolve_command_with_home_and_path("codex", &temp, "");
        assert_eq!(resolved.as_deref(), Some(exe.as_path()));

        let _ = std::fs::remove_dir_all(temp);
    }
}
```

- [ ] **Step 2: Run tests and confirm failure**

Run:

```bash
cargo test -p heiwa_provider command_resolution_tests -- --nocapture
```

Expected: fail because `provider_search_paths_for_home` / `resolve_command_with_home_and_path` do not exist.

- [ ] **Step 3: Implement resolver**

Add helpers in `crates/heiwa_provider/src/lib.rs`:

```rust
fn provider_search_paths_for_home(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".heiwa").join("bin"),
        home.join(".local").join("bin"),
        home.join(".npm-global").join("bin"),
        home.join(".cargo").join("bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]
}

pub fn resolve_command(cmd: &str) -> Option<PathBuf> {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let path = env::var("PATH").unwrap_or_default();
    resolve_command_with_home_and_path(cmd, &home, &path)
}

fn resolve_command_with_home_and_path(cmd: &str, home: &Path, path: &str) -> Option<PathBuf> {
    let mut dirs = env::split_paths(path).collect::<Vec<_>>();
    dirs.extend(provider_search_paths_for_home(home));
    dirs.into_iter()
        .map(|dir| dir.join(cmd))
        .find(|candidate| candidate.is_file())
}

fn has_command(cmd: &str) -> bool {
    resolve_command(cmd).is_some()
}
```

- [ ] **Step 4: Make provider adapters use resolved binaries**

In each CLI adapter, replace:

```rust
let mut cmd = Command::new("claude");
```

with:

```rust
let binary = heiwa_provider::resolve_command("claude").unwrap_or_else(|| PathBuf::from("claude"));
let mut cmd = Command::new(binary);
```

Use `codex` and `gemini` in the matching files. Add `use std::path::PathBuf;` only where needed.

- [ ] **Step 5: Verify provider parity from a launchd-like PATH**

Run:

```bash
PATH="/Users/dmcgregsauce/.heiwa/bin:/Users/dmcgregsauce/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin" cargo test -p heiwa_provider command_resolution_tests -- --nocapture
PATH="/Users/dmcgregsauce/.heiwa/bin:/Users/dmcgregsauce/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin" cargo run -q -p heiwa-shell -- providers
```

Expected: tests pass; `claude`, `codex`, and `gemini` are no longer reported as `not_installed` solely because launchd PATH is narrow.

- [ ] **Step 6: Commit**

```bash
git add crates/heiwa_provider/src/lib.rs crates/heiwa_provider/src/providers/claude_code.rs crates/heiwa_provider/src/providers/codex_cli.rs crates/heiwa_provider/src/providers/gemini_cli.rs
git commit -m "fix: resolve provider CLIs in private server runtime"
```

---

### Task 2: Local Resource Policy Crate

**Files:**
- Create: `crates/heiwa_resource/Cargo.toml`
- Create: `crates/heiwa_resource/src/lib.rs`
- Create: `crates/heiwa_resource/tests/policy.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add failing policy tests**

Create `crates/heiwa_resource/tests/policy.rs`:

```rust
use heiwa_resource::{Admission, ResourcePolicy, ResourceSnapshot, WorkClass};

fn base() -> ResourceSnapshot {
    ResourceSnapshot {
        cpu_count: 12,
        load_1m: 3.0,
        free_memory_bytes: 10 * 1024 * 1024 * 1024,
        battery_percent: Some(80),
        on_battery: false,
        thermal_pressure: "nominal".to_string(),
    }
}

#[test]
fn allows_local_summary_when_machine_is_healthy() {
    let policy = ResourcePolicy::default();
    let decision = policy.admit(&base(), WorkClass::LocalSummary);
    assert_eq!(decision.admission, Admission::Allow);
}

#[test]
fn throttles_background_when_load_is_high() {
    let policy = ResourcePolicy::default();
    let mut snapshot = base();
    snapshot.load_1m = 10.5;
    let decision = policy.admit(&snapshot, WorkClass::BackgroundLocalModel);
    assert_eq!(decision.admission, Admission::Throttle);
    assert!(decision.reason.contains("load"));
}

#[test]
fn denies_background_when_low_battery() {
    let policy = ResourcePolicy::default();
    let mut snapshot = base();
    snapshot.on_battery = true;
    snapshot.battery_percent = Some(18);
    let decision = policy.admit(&snapshot, WorkClass::BackgroundLocalModel);
    assert_eq!(decision.admission, Admission::Deny);
    assert!(decision.reason.contains("battery"));
}
```

- [ ] **Step 2: Run tests and confirm failure**

Run:

```bash
cargo test -p heiwa_resource
```

Expected: fail because the crate is not yet in the workspace.

- [ ] **Step 3: Add crate and pure policy**

Create `crates/heiwa_resource/src/lib.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceSnapshot {
    pub cpu_count: usize,
    pub load_1m: f64,
    pub free_memory_bytes: u64,
    pub battery_percent: Option<u8>,
    pub on_battery: bool,
    pub thermal_pressure: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkClass {
    LocalSummary,
    BackgroundLocalModel,
    Embedding,
    ProviderCli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Allow,
    Throttle,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionDecision {
    pub admission: Admission,
    pub reason: String,
    pub max_concurrency: usize,
    pub poll_interval_seconds: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourcePolicy {
    pub load_soft_ratio: f64,
    pub load_hard_ratio: f64,
    pub min_free_memory_bytes: u64,
    pub low_battery_percent: u8,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            load_soft_ratio: 0.70,
            load_hard_ratio: 0.90,
            min_free_memory_bytes: 6 * 1024 * 1024 * 1024,
            low_battery_percent: 25,
        }
    }
}

impl ResourcePolicy {
    pub fn admit(&self, snapshot: &ResourceSnapshot, work: WorkClass) -> AdmissionDecision {
        let load_ratio = snapshot.load_1m / snapshot.cpu_count.max(1) as f64;
        if matches!(work, WorkClass::BackgroundLocalModel)
            && snapshot.on_battery
            && snapshot.battery_percent.unwrap_or(100) <= self.low_battery_percent
        {
            return deny("low battery", 0, 3600);
        }
        if snapshot.free_memory_bytes < self.min_free_memory_bytes {
            return throttle("low free memory", 1, 1800);
        }
        if load_ratio >= self.load_hard_ratio {
            return deny("load hard cap", 0, 3600);
        }
        if load_ratio >= self.load_soft_ratio {
            return throttle("load soft cap", 1, 1800);
        }
        AdmissionDecision {
            admission: Admission::Allow,
            reason: "healthy".to_string(),
            max_concurrency: match work {
                WorkClass::Embedding => 1,
                WorkClass::BackgroundLocalModel => 1,
                WorkClass::LocalSummary => 2,
                WorkClass::ProviderCli => 2,
            },
            poll_interval_seconds: 300,
        }
    }
}

fn throttle(reason: &str, max_concurrency: usize, poll_interval_seconds: u64) -> AdmissionDecision {
    AdmissionDecision {
        admission: Admission::Throttle,
        reason: reason.to_string(),
        max_concurrency,
        poll_interval_seconds,
    }
}

fn deny(reason: &str, max_concurrency: usize, poll_interval_seconds: u64) -> AdmissionDecision {
    AdmissionDecision {
        admission: Admission::Deny,
        reason: reason.to_string(),
        max_concurrency,
        poll_interval_seconds,
    }
}
```

Add `crates/heiwa_resource` to the root workspace members.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test -p heiwa_resource
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/heiwa_resource
git commit -m "feat: add local resource admission policy"
```

---

### Task 3: Runtime Resource Snapshot and API

**Files:**
- Modify: `apps/heiwa_shell/Cargo.toml`
- Modify: `apps/heiwa_shell/src/cmd/app.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Test: `apps/heiwa_shell/tests/smoke.rs`

- [ ] **Step 1: Add failing API smoke test**

In `apps/heiwa_shell/tests/smoke.rs`, add:

```rust
#[test]
fn app_resource_payload_reports_private_server_policy() {
    let payload = heiwa_shell::cmd::app::test_api_payload("/api/v1/resource");
    assert_eq!(payload["ok"].as_bool(), Some(true));
    assert!(payload["data"]["snapshot"]["cpu_count"].as_u64().unwrap() >= 1);
    assert!(payload["data"]["decision"]["reason"].as_str().is_some());
}
```

If `test_api_payload` is not public today, add a `pub(crate)` test helper behind `#[cfg(test)]` rather than exposing internals publicly.

- [ ] **Step 2: Run test and confirm failure**

Run:

```bash
cargo test -p heiwa-shell app_resource_payload_reports_private_server_policy
```

Expected: fail because `/api/v1/resource` does not exist.

- [ ] **Step 3: Implement macOS-safe snapshot collection**

In `apps/heiwa_shell/src/cmd/app.rs`, add a small local function that builds `heiwa_resource::ResourceSnapshot` with safe defaults:

```rust
fn resource_snapshot() -> heiwa_resource::ResourceSnapshot {
    heiwa_resource::ResourceSnapshot {
        cpu_count: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        load_1m: load_average_1m().unwrap_or(0.0),
        free_memory_bytes: free_memory_bytes().unwrap_or(0),
        battery_percent: battery_percent(),
        on_battery: on_battery_power(),
        thermal_pressure: "unknown".to_string(),
    }
}
```

Use no privileged commands. If a probe fails, return `unknown` / `0` and let the policy degrade conservatively.

- [ ] **Step 4: Expose `/api/v1/resource`**

Add to `api_payload`:

```rust
"/api/v1/resource" => {
    let snapshot = resource_snapshot();
    let policy = heiwa_resource::ResourcePolicy::default();
    let decision = policy.admit(&snapshot, heiwa_resource::WorkClass::BackgroundLocalModel);
    json!({
        "ok": true,
        "data": {
            "snapshot": snapshot_to_json(&snapshot),
            "decision": {
                "admission": format!("{:?}", decision.admission).to_lowercase(),
                "reason": decision.reason,
                "max_concurrency": decision.max_concurrency,
                "poll_interval_seconds": decision.poll_interval_seconds
            }
        }
    })
}
```

- [ ] **Step 5: Gate route preview metadata**

In route preview output, include local resource admission when the selected provider is `ollama`.

Expected JSON metadata:

```json
{
  "local_resource": {
    "admission": "allow",
    "reason": "healthy",
    "max_concurrency": 1
  }
}
```

- [ ] **Step 6: Run verification**

Run:

```bash
cargo test -p heiwa-shell app_resource_payload_reports_private_server_policy
cargo run -q -p heiwa-shell -- app start --port 7475 --no-open
/usr/bin/curl -fsS http://127.0.0.1:7475/api/v1/resource | jq .
cargo run -q -p heiwa-shell -- route preview 'private server resource check' --json
```

Expected: resource API returns JSON; route preview still prefers Ollama for status/routine tasks and includes local-resource metadata.

- [ ] **Step 7: Stop temporary runtime and commit**

Stop the `7475` process started in Step 6.

```bash
git add apps/heiwa_shell/Cargo.toml apps/heiwa_shell/src/cmd/app.rs apps/heiwa_shell/src/main.rs apps/heiwa_shell/tests/smoke.rs
git commit -m "feat: surface private server resource policy"
```

---

### Task 4: Local Agent Bus

**Files:**
- Create: `crates/heiwa_agent_bus/Cargo.toml`
- Create: `crates/heiwa_agent_bus/src/lib.rs`
- Create: `crates/heiwa_agent_bus/tests/spool.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add failing spool tests**

Create `crates/heiwa_agent_bus/tests/spool.rs`:

```rust
use heiwa_agent_bus::{AgentBus, AgentRecord, AgentStatus};

#[test]
fn registers_agent_and_lists_it() {
    let dir = std::env::temp_dir().join(format!("heiwa-agent-bus-{}", std::process::id()));
    let bus = AgentBus::open(&dir).unwrap();
    bus.register(AgentRecord {
        agent_id: "ollama@local".to_string(),
        provider: Some("ollama".to_string()),
        class: "summary_small".to_string(),
        status: AgentStatus::Idle,
        last_seen_utc: "2026-06-02T00:00:00Z".to_string(),
    }).unwrap();
    let agents = bus.list_agents().unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].agent_id, "ollama@local");
    let _ = std::fs::remove_dir_all(dir);
}
```

- [ ] **Step 2: Run test and confirm failure**

Run:

```bash
cargo test -p heiwa_agent_bus
```

Expected: fail because crate is absent.

- [ ] **Step 3: Implement minimal local bus**

State layout:

```text
~/.heiwa/state/agents/registry.json
~/.heiwa/state/agents/tasks.jsonl
~/.heiwa/state/agents/status.jsonl
```

Public API:

```rust
pub struct AgentBus { root: PathBuf }
pub struct AgentRecord { agent_id, provider, class, status, last_seen_utc }
pub enum AgentStatus { Idle, Working, Blocked, Offline }

impl AgentBus {
    pub fn open(root: impl AsRef<Path>) -> Result<Self>;
    pub fn default() -> Result<Self>;
    pub fn register(&self, record: AgentRecord) -> Result<()>;
    pub fn list_agents(&self) -> Result<Vec<AgentRecord>>;
    pub fn append_task(&self, task: &heiwa_a2a::Task) -> Result<()>;
    pub fn list_tasks(&self) -> Result<Vec<heiwa_a2a::Task>>;
    pub fn append_status(&self, event: &heiwa_a2a::StatusEvent) -> Result<()>;
}
```

Use append-only JSONL for tasks/status and pretty JSON for registry. Do not spawn provider CLIs here; this crate only coordinates state.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test -p heiwa_agent_bus
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/heiwa_agent_bus
git commit -m "feat: add local agent coordination bus"
```

---

### Task 5: `heiwa agents` CLI

**Files:**
- Create: `apps/heiwa_shell/src/cmd/agents.rs`
- Modify: `apps/heiwa_shell/src/cmd/mod.rs`
- Modify: `apps/heiwa_shell/src/cli.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `apps/heiwa_shell/Cargo.toml`
- Test: `apps/heiwa_shell/tests/smoke.rs`

- [ ] **Step 1: Add failing CLI smoke tests**

Add tests for:

```bash
heiwa agents list --json
heiwa agents register --id ollama@local --provider ollama --class summary_small --json
heiwa agents send --from devon@local --to ollama@local --text "status check" --json
heiwa agents inbox --json
```

Expected properties:

- `list` returns `{"command":"agents list","agents":[]}` on empty state.
- `register` writes only under `HEIWA_STATE_DIR` when set.
- `send` writes a `heiwa_a2a::Task`.
- `inbox` lists the task without executing it.

- [ ] **Step 2: Run tests and confirm failure**

Run:

```bash
cargo test -p heiwa-shell agents_
```

Expected: fail because command is missing.

- [ ] **Step 3: Implement command**

`apps/heiwa_shell/src/cmd/agents.rs` responsibilities:

- parse `list`, `register`, `send`, `inbox`
- open `heiwa_agent_bus::AgentBus::default()`
- never perform external side effects
- print JSON when `--json` is present

Use `heiwa_a2a::Task::new(WorkerClass::SummarySmall)` for `send`, set `from`, `assignee`, and one text message.

- [ ] **Step 4: Wire command and help**

Add:

```rust
pub mod agents;
```

to `cmd/mod.rs`, dispatch in `cli.rs`, and add help line:

```text
agents list|register|send|inbox  Coordinate local provider agents
```

- [ ] **Step 5: Verify**

Run:

```bash
HEIWA_STATE_DIR=/tmp/heiwa-agent-test cargo run -q -p heiwa-shell -- agents list --json
HEIWA_STATE_DIR=/tmp/heiwa-agent-test cargo run -q -p heiwa-shell -- agents register --id ollama@local --provider ollama --class summary_small --json
HEIWA_STATE_DIR=/tmp/heiwa-agent-test cargo run -q -p heiwa-shell -- agents send --from devon@local --to ollama@local --text "status check" --json
HEIWA_STATE_DIR=/tmp/heiwa-agent-test cargo run -q -p heiwa-shell -- agents inbox --json
cargo test -p heiwa-shell agents_
```

Expected: commands work and no network/public side effects occur.

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_shell/Cargo.toml apps/heiwa_shell/src/cmd/agents.rs apps/heiwa_shell/src/cmd/mod.rs apps/heiwa_shell/src/cli.rs apps/heiwa_shell/src/main.rs apps/heiwa_shell/tests/smoke.rs
git commit -m "feat: add local agent bus CLI"
```

---

### Task 6: Private Server API and Cockpit State

**Files:**
- Modify: `apps/heiwa_shell/src/cmd/app.rs`
- Modify: `apps/heiwa_shell/tests/smoke.rs`
- Modify: `apps/heiwa_app/clients/cockpit/src/lib/types.ts`
- Modify: `apps/heiwa_app/clients/cockpit/src/lib/endpoints.ts`
- Modify: `apps/heiwa_app/clients/cockpit/src/routes/Today.tsx`

- [ ] **Step 1: Add failing private server API test**

Expected payload for `/api/v1/private-server`:

```json
{
  "ok": true,
  "data": {
    "mode": "private_localhost",
    "bind": "127.0.0.1",
    "external_exposure": false,
    "providers": { "connected": 4, "missing": ["antigravity"] },
    "local_models": { "provider": "ollama", "admission": "allow" },
    "agents": { "registered": 0, "pending_tasks": 0 },
    "approvals": { "pending": 0 }
  }
}
```

- [ ] **Step 2: Run test and confirm failure**

Run:

```bash
cargo test -p heiwa-shell private_server_payload
```

Expected: fail because endpoint is missing.

- [ ] **Step 3: Implement endpoint**

In `api_payload`, add `/api/v1/private-server` by composing existing helpers:

- `provider_rows()`
- `resource_snapshot()`
- `heiwa_agent_bus::AgentBus::default().list_agents()`
- `approvals_summary(&state_dir())`

The endpoint must never include secrets, tokens, raw prompts, or provider config file contents.

- [ ] **Step 4: Add cockpit types/endpoints**

Add TypeScript types:

```ts
export type PrivateServerState = {
  mode: 'private_localhost'
  bind: string
  external_exposure: boolean
  providers: { connected: number; missing: string[] }
  local_models: { provider: string; admission: string; reason: string }
  agents: { registered: number; pending_tasks: number }
  approvals: { pending: number }
}
```

Add endpoint constant for `/api/v1/private-server`.

- [ ] **Step 5: Render minimal status panel**

In `Today.tsx`, add a compact panel:

```text
Private Server: localhost-only
Local model admission: allow/throttle/deny
Provider agents: connected/missing
Agent bus: registered / pending
Approvals: pending
```

Do not redesign the cockpit.

- [ ] **Step 6: Verify installed-vs-checkout on alternate port**

Run:

```bash
cargo test -p heiwa-shell private_server_payload
pnpm --dir apps/heiwa_app/clients/cockpit test -- --run
cargo run -q -p heiwa-shell -- app start --port 7475 --no-open
/usr/bin/curl -fsS http://127.0.0.1:7475/api/v1/private-server | jq .
```

Stop the `7475` runtime.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_shell/src/cmd/app.rs apps/heiwa_shell/tests/smoke.rs apps/heiwa_app/clients/cockpit/src/lib/types.ts apps/heiwa_app/clients/cockpit/src/lib/endpoints.ts apps/heiwa_app/clients/cockpit/src/routes/Today.tsx
git commit -m "feat: show private server status in cockpit"
```

---

### Task 7: Orchestrator Resource-Aware Agent Heartbeats

**Files:**
- Modify: `apps/heiwa_orchestrator/Cargo.toml`
- Modify: `apps/heiwa_orchestrator/src/runtime/mod.rs`
- Modify: `apps/heiwa_orchestrator/src/config.rs`
- Test: `apps/heiwa_orchestrator/src/runtime/mod.rs`

- [ ] **Step 1: Add failing orchestrator tests**

Test that `runtime::plan_tick`:

- registers `orchestrator@local`
- returns `poll_interval_seconds` from `heiwa_resource`
- does not schedule background local model work when resource admission is `Deny`

Sketch:

```rust
#[test]
fn tick_denies_background_work_under_pressure() {
    let snapshot = ResourceSnapshot {
        cpu_count: 12,
        load_1m: 11.0,
        free_memory_bytes: 8 * 1024 * 1024 * 1024,
        battery_percent: Some(90),
        on_battery: false,
        thermal_pressure: "nominal".to_string(),
    };
    let plan = plan_tick(&snapshot, &ResourcePolicy::default());
    assert!(!plan.allow_background_local_model);
    assert!(plan.poll_interval_seconds >= 1800);
}
```

- [ ] **Step 2: Run test and confirm failure**

Run:

```bash
cargo test -p heiwa-orchestrator tick_denies_background_work_under_pressure
```

Expected: fail because planning API does not exist.

- [ ] **Step 3: Implement tick planner**

Add:

```rust
pub struct TickPlan {
    pub allow_background_local_model: bool,
    pub poll_interval_seconds: u64,
    pub reason: String,
}

pub fn plan_tick(snapshot: &ResourceSnapshot, policy: &ResourcePolicy) -> TickPlan {
    let decision = policy.admit(snapshot, WorkClass::BackgroundLocalModel);
    TickPlan {
        allow_background_local_model: matches!(decision.admission, Admission::Allow),
        poll_interval_seconds: decision.poll_interval_seconds,
        reason: decision.reason,
    }
}
```

- [ ] **Step 4: Make runtime loop local-only and state-backed**

Update `run(cfg)` so it:

- opens `AgentBus::default()`
- registers `orchestrator@local`
- writes heartbeat/status
- sleeps according to resource policy
- does not call external providers, browser, or network in v0

- [ ] **Step 5: Verify**

Run:

```bash
cargo test -p heiwa-orchestrator
cargo run -q -p heiwa-orchestrator
```

For manual run, interrupt after first tick. Confirm `~/.heiwa/state/agents/registry.json` includes `orchestrator@local`.

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_orchestrator/Cargo.toml apps/heiwa_orchestrator/src/runtime/mod.rs apps/heiwa_orchestrator/src/config.rs
git commit -m "feat: make orchestrator resource-aware"
```

---

### Task 8: Private Server Operator Contract

**Files:**
- Create: `docs/local-private-server.md`
- Modify: `docs/local-self-operation.md`
- Modify: `HEIWA.md`

- [ ] **Step 1: Write the doc**

Create `docs/local-private-server.md` with:

```markdown
# Local Private Server Mode

Heiwa runs as Devon's private localhost AI server on the MacBook.

Non-negotiables:

- Bind app/runtime APIs to `127.0.0.1` by default.
- Do not expose the app through Cloudflare, public DNS, ngrok, Tailscale Funnel, or reverse proxies unless a future explicit approval changes the mode.
- Prefer Ollama/local models for routine, private, summarization, and status work.
- Throttle or deny background local model work under high load, low memory, battery pressure, or thermal pressure.
- Use provider CLIs as escalation lanes under provider-owned auth and quota.
- Provider agents communicate through the local Heiwa agent bus.
- External side effects stay approval-gated.

Useful probes:

```bash
heiwa app runtime status --json
heiwa providers
heiwa route preview "status check" --json
heiwa agents list --json
/usr/bin/curl -fsS http://127.0.0.1:7474/api/v1/private-server | jq .
```
```

- [ ] **Step 2: Link it**

Add links from:

- `docs/local-self-operation.md`
- `HEIWA.md`

- [ ] **Step 3: Verify docs**

Run:

```bash
.venv/bin/mkdocs build --strict
```

If the local docs venv is unavailable, report that and run:

```bash
rg -n "local-private-server|Private Server Mode" HEIWA.md docs/local-self-operation.md docs/local-private-server.md
```

- [ ] **Step 4: Commit**

```bash
git add HEIWA.md docs/local-self-operation.md docs/local-private-server.md
git commit -m "docs: define local private server mode"
```

---

### Task 9: Checkout Runtime Verification and Installed Promotion

**Files:**
- No source edits unless verification reveals defects.

- [ ] **Step 1: Run focused tests**

Run:

```bash
cargo test -p heiwa_provider
cargo test -p heiwa_resource
cargo test -p heiwa_agent_bus
cargo test -p heiwa-shell agents_ private_server resource
cargo test -p heiwa-orchestrator
```

Expected: all pass.

- [ ] **Step 2: Verify checkout runtime on alternate port**

Run:

```bash
cargo run -q -p heiwa-shell -- app start --port 7475 --no-open
/usr/bin/curl -fsS http://127.0.0.1:7475/status/health | jq .
/usr/bin/curl -fsS http://127.0.0.1:7475/api/v1/private-server | jq .
/usr/bin/curl -fsS http://127.0.0.1:7475/api/v1/resource | jq .
/usr/bin/curl -fsS http://127.0.0.1:7475/api/v1/agents | jq .
```

Expected: all endpoints return JSON, not `index.html`. Stop the `7475` runtime.

- [ ] **Step 3: Dry-run installed promotion**

Run:

```bash
cargo run -q -p heiwa-shell -- app update --source checkout --dry-run
```

Expected: shows checkout source and install target under `~/.heiwa/bin/heiwa`.

- [ ] **Step 4: Promote only after approval**

This changes the installed runtime, so it needs Devon approval.

```bash
cargo run -q -p heiwa-shell -- app update --source checkout
```

- [ ] **Step 5: Restart policy**

If installed promotion happens, restart `ltd.heiwa.app` only after confirming:

```bash
heiwa approvals list --json
heiwa workers status --json
```

Expected: no pending approvals and no active blocking task.

- [ ] **Step 6: Final installed truth**

Run:

```bash
heiwa app runtime status --json
/usr/bin/curl -fsS http://127.0.0.1:7474/api/v1/private-server | jq .
heiwa providers
heiwa route preview "private server final status" --json
```

Expected:

- app binds to `127.0.0.1`
- provider state matches shell state for Claude/Codex/Gemini/Ollama
- local route still selects Ollama for status/routine work
- resource policy is visible
- agent bus is visible

- [ ] **Step 7: Commit verification notes if docs changed**

Only commit source/docs changes, not `~/.heiwa/state` artifacts.

---

## Execution Notes

- Use local models for routine summarization, status, compression, and first-pass classification.
- Do not keep more than one heavy Ollama model warm for background work in v0.
- Use embeddings only when needed and keep embedding work single-concurrency.
- Escalate to Gemini/Claude/Codex only when local admission denies, local model class is insufficient, or the task is explicitly code/review/frontier.
- Browserbase CLI is reference/optional. Default browser automation is local-only; cloud Browserbase functions are out of scope.
- All provider agents are peers, but Heiwa owns the coordination bus, leases, approvals, and receipts.
- No web exposure is allowed in this plan.
