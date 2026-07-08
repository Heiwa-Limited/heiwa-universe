# Heiwa Tmux Multiplexer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Rust-native, tmux-backed local multiplexer so Heiwa can own live worker terminals under session-scoped leases.

**Architecture:** Keep tmux as the local terminal substrate, but keep Heiwa as the authority. A new `heiwa_terminal` crate owns private tmux sockets under `~/.heiwa/state/terminals/`, exposes launch/list/send/read/close/cleanup APIs, and returns bounded structured events for shell/app surfaces. The CLI gets a deterministic `heiwa terminal ...` surface first; app attach and worker-provider integration come later.

**Tech Stack:** Rust std process/fs APIs, tmux, serde/serde_json, existing `heiwa_install::get_heiwa_dir()`.

**Dependency rule:** First slice must not add frontend libraries or extra Rust
crates beyond serialization plus existing Heiwa crates. Generate terminal ids
with std-only process/time material until a real collision or ordering problem
proves another dependency is needed.

---

## Source Inputs

- OmniGent clone: `/Users/dmcgregsauce/oss-repos/omnigent`
- OmniGent lift note: `oss-lifts/omnigent/README.md`
- OmniGent terminal refs:
  - `omnigent/terminals/registry.py`
  - `omnigent/terminals/ws_bridge.py`
  - `omnigent/inner/terminal.py`
  - `omnigent/tools/builtins/sys_terminal.py`
  - `docs/AGENT_YAML_SPEC.md`
- Heiwa current target note: `docs/architecture/app-foundation.md` already lists `Local multiplexer: sessions, workers, PTY/log tail, pause/resume`.

## File Structure

### New files

| File                                      | Responsibility                                                                      |
| ----------------------------------------- | ----------------------------------------------------------------------------------- |
| `crates/heiwa_terminal/Cargo.toml`        | New crate manifest.                                                                 |
| `crates/heiwa_terminal/src/lib.rs`        | Public API exports.                                                                 |
| `crates/heiwa_terminal/src/spec.rs`       | Terminal spec, ids, launch options.                                                 |
| `crates/heiwa_terminal/src/tmux.rs`       | Small tmux command wrapper: availability, launch, send, capture, kill, has-session. |
| `crates/heiwa_terminal/src/registry.rs`   | Session-scoped registry and private socket lifecycle.                               |
| `crates/heiwa_terminal/src/receipt.rs`    | Bounded JSON receipt writer under terminal state.                                   |
| `crates/heiwa_terminal/tests/registry.rs` | Hermetic private-socket tmux tests.                                                 |
| `apps/heiwa_shell/src/cmd/terminal.rs`    | `heiwa terminal` CLI command.                                                       |
| `apps/heiwa_shell/tests/terminal.rs`      | CLI integration tests with temp `HOME`.                                             |

### Modified files

| File                              | What changes                                                                                    |
| --------------------------------- | ----------------------------------------------------------------------------------------------- |
| `Cargo.toml`                      | Add `crates/heiwa_terminal` workspace member.                                                   |
| `apps/heiwa_shell/Cargo.toml`     | Add `heiwa-terminal` dependency.                                                                |
| `apps/heiwa_shell/src/cmd/mod.rs` | Add `pub mod terminal;`.                                                                        |
| `apps/heiwa_shell/src/cli.rs`     | Route `heiwa terminal ...`.                                                                     |
| `apps/heiwa_shell/src/main.rs`    | Add help line; include tmux in doctor output.                                                   |
| `crates/heiwa_install/src/lib.rs` | Add `tmux_version` and `tmux_installed` fields to doctor reports; add AI-ops multiplexer check. |

---

### Task 1: Crate Skeleton And Tmux Probe

**Files:**

- Create: `crates/heiwa_terminal/Cargo.toml`
- Create: `crates/heiwa_terminal/src/lib.rs`
- Create: `crates/heiwa_terminal/src/spec.rs`
- Create: `crates/heiwa_terminal/src/tmux.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add the crate manifest**

```toml
[package]
name = "heiwa-terminal"
version = "0.1.0"
edition = "2021"
description = "tmux-backed local terminal multiplexer for Heiwa"
license.workspace = true
repository.workspace = true
homepage.workspace = true
documentation.workspace = true
readme.workspace = true
keywords.workspace = true
categories.workspace = true

[lib]
name = "heiwa_terminal"
path = "src/lib.rs"

[dependencies]
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
heiwa-install = { path = "../heiwa_install" }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Add workspace member**

In `/Users/dmcgregsauce/heiwa-universe/Cargo.toml`, add:

```toml
"crates/heiwa_terminal",
```

Place it near other `crates/heiwa_*` members.

- [ ] **Step 3: Add public exports**

In `crates/heiwa_terminal/src/lib.rs`:

```rust
pub mod registry;
pub mod receipt;
pub mod spec;
pub mod tmux;

pub use registry::TerminalRegistry;
pub use spec::{TerminalId, TerminalLaunchOptions, TerminalSpec};
```

- [ ] **Step 4: Define terminal ids/specs**

In `crates/heiwa_terminal/src/spec.rs`, define:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalId {
    pub session_id: String,
    pub name: String,
    pub instance_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSpec {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd_root: Option<PathBuf>,
    pub allow_cwd_override: bool,
    pub scrollback: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalLaunchOptions {
    pub cwd: Option<PathBuf>,
}
```

- [ ] **Step 5: Add tmux availability helper**

In `crates/heiwa_terminal/src/tmux.rs`, implement:

```rust
use anyhow::{anyhow, Result};
use std::process::Command;

pub fn tmux_version() -> Option<String> {
    let out = Command::new("tmux").arg("-V").output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn require_tmux() -> Result<String> {
    tmux_version().ok_or_else(|| anyhow!("tmux is required for Heiwa local multiplexer"))
}
```

- [ ] **Step 6: Add failing/proving unit tests**

Add tests in `tmux.rs`:

```rust
#[test]
fn tmux_probe_is_structured() {
    let version = tmux_version();
    if let Some(version) = version {
        assert!(version.starts_with("tmux "));
    }
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p heiwa-terminal tmux_probe_is_structured`

Expected: PASS on Devon's MacBook (`tmux 3.6a` currently installed). On a machine without tmux, this unit test may pass with `None`; later CLI/doctor checks fail loud where tmux is required.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/heiwa_terminal
git commit -m "feat: add heiwa_terminal crate with tmux probe"
```

---

### Task 2: Private Tmux Registry

**Files:**

- Create: `crates/heiwa_terminal/src/registry.rs`
- Expand: `crates/heiwa_terminal/src/tmux.rs`
- Test: `crates/heiwa_terminal/tests/registry.rs`

- [ ] **Step 1: Write failing launch/read/close test**

Create `crates/heiwa_terminal/tests/registry.rs`:

```rust
use heiwa_terminal::{TerminalLaunchOptions, TerminalRegistry, TerminalSpec};

#[test]
fn launch_send_read_close_round_trip() {
    if heiwa_terminal::tmux::tmux_version().is_none() {
        eprintln!("skipping: tmux not installed");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let registry = TerminalRegistry::new(temp.path().to_path_buf());
    let spec = TerminalSpec {
        name: "shell".into(),
        command: "bash".into(),
        args: vec!["-lc".into(), "read line; printf \"got:%s\" \"$line\"; sleep 2".into()],
        env: vec![],
        cwd_root: Some(temp.path().to_path_buf()),
        allow_cwd_override: true,
        scrollback: 2000,
    };

    let id = registry
        .launch("test-session", &spec, "main", TerminalLaunchOptions { cwd: None })
        .unwrap();
    registry.send_text(&id, "hello", true).unwrap();
    let screen = registry.read(&id, 20).unwrap().screen;
    assert!(screen.contains("got:hello"), "{screen}");
    assert!(registry.close(&id).unwrap());
    assert!(!registry.close(&id).unwrap());
}
```

- [ ] **Step 2: Implement private state layout**

In `registry.rs`, use this layout:

```text
<root>/<session_id>/<name>/<instance_id>/
  owner.pid
  terminal.json
  tmux.sock
```

`root` is provided by tests and by production `terminal_state_root()`.

- [ ] **Step 3: Implement `TerminalRegistry`**

Required API:

```rust
pub struct TerminalRegistry {
    root: PathBuf,
}

impl TerminalRegistry {
    pub fn new(root: PathBuf) -> Self;
    pub fn default_state_root() -> PathBuf;
    pub fn launch(
        &self,
        session_id: &str,
        spec: &TerminalSpec,
        instance_id: &str,
        options: TerminalLaunchOptions,
    ) -> Result<TerminalId>;
    pub fn list(&self, session_id: Option<&str>) -> Result<Vec<TerminalSummary>>;
    pub fn send_text(&self, id: &TerminalId, text: &str, enter: bool) -> Result<()>;
    pub fn send_keys(&self, id: &TerminalId, keys: &[String]) -> Result<()>;
    pub fn read(&self, id: &TerminalId, scrollback: usize) -> Result<TerminalCapture>;
    pub fn close(&self, id: &TerminalId) -> Result<bool>;
    pub fn cleanup_session(&self, session_id: &str) -> Result<usize>;
    pub fn reap_orphans(&self) -> Result<usize>;
}
```

- [ ] **Step 4: Implement tmux wrapper commands**

In `tmux.rs`, use `Command::new("tmux")` with explicit args:

- launch: `tmux -S <sock> -f /dev/null set-option ... ; new-session -d -s main -x 80 -y 24 -c <cwd> <command>`
- send literal: `tmux -S <sock> -f /dev/null send-keys -l -t main <text>`
- send key: `tmux -S <sock> -f /dev/null send-keys -t main <key>`
- read: `tmux -S <sock> -f /dev/null capture-pane -t main -p [-S -N]`
- alive: `tmux -S <sock> -f /dev/null has-session -t main`
- close: `tmux -S <sock> -f /dev/null kill-server`

Do not pass `OMNIGENT_TMUX_SOCK` or any Heiwa tmux socket env into pane processes.

- [ ] **Step 5: Add cwd containment guard**

If `options.cwd` exists:

- require `spec.allow_cwd_override == true`;
- resolve against `spec.cwd_root` when relative;
- reject any resolved cwd outside `spec.cwd_root`.

Expected error text contains: `cwd override escapes terminal root`.

- [ ] **Step 6: Add idempotent close test**

Extend `registry.rs` test to assert second close returns `false`, not error.

- [ ] **Step 7: Add orphan sweep test**

Write a test that creates fake stale state with `owner.pid = 0`; `reap_orphans()` removes it without touching active test sessions.

- [ ] **Step 8: Run tests**

Run: `cargo test -p heiwa-terminal`

Expected: PASS. If tmux missing, integration round-trip tests skip with explicit stderr.

- [ ] **Step 9: Commit**

```bash
git add crates/heiwa_terminal
git commit -m "feat: add private tmux terminal registry"
```

---

### Task 3: Terminal Action Receipts

**Files:**

- Create: `crates/heiwa_terminal/src/receipt.rs`
- Modify: `crates/heiwa_terminal/src/registry.rs`
- Test: `crates/heiwa_terminal/tests/registry.rs`

- [ ] **Step 1: Define receipt shape**

In `receipt.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalReceipt {
    pub receipt_id: String,
    pub created_at_unix_ms: u128,
    pub action: String,
    pub session_id: String,
    pub terminal_name: String,
    pub instance_id: String,
    pub status: String,
    pub cwd: Option<String>,
    pub command: Option<String>,
    pub preview: Option<String>,
    pub error: Option<String>,
}
```

- [ ] **Step 2: Write bounded JSON receipts**

Store receipts under:

```text
~/.heiwa/state/terminals/receipts/<receipt_id>.json
```

For tests, receipts use the registry root:

```text
<root>/receipts/<receipt_id>.json
```

Do not store full terminal scrollback by default. `read` receipts may include a max 240-character stripped preview.

- [ ] **Step 3: Emit receipts for launch/send/read/close**

Each registry method writes a receipt on success and on known failure. For send, record `text_len` in preview or metadata, not raw text.

- [ ] **Step 4: Add receipt tests**

Extend registry tests:

```rust
let receipts = std::fs::read_dir(temp.path().join("receipts")).unwrap().count();
assert!(receipts >= 4);
```

Parse one receipt and assert `session_id == "test-session"` and `action == "terminal.launch"`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p heiwa-terminal receipt`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/heiwa_terminal
git commit -m "feat: record terminal lifecycle receipts"
```

---

### Task 4: `heiwa terminal` CLI

**Files:**

- Create: `apps/heiwa_shell/src/cmd/terminal.rs`
- Modify: `apps/heiwa_shell/Cargo.toml`
- Modify: `apps/heiwa_shell/src/cmd/mod.rs`
- Modify: `apps/heiwa_shell/src/cli.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Test: `apps/heiwa_shell/tests/terminal.rs`

- [ ] **Step 1: Add dependency**

In `apps/heiwa_shell/Cargo.toml`:

```toml
heiwa-terminal = { path = "../../crates/heiwa_terminal" }
```

- [ ] **Step 2: Route command module**

In `apps/heiwa_shell/src/cmd/mod.rs`:

```rust
pub mod terminal;
```

In `apps/heiwa_shell/src/cli.rs`:

```rust
Some("terminal") => {
    cmd::terminal::run(&args[2..])?;
    Ok(true)
}
```

Add help in `apps/heiwa_shell/src/main.rs`:

```rust
println!("  terminal ...                  Manage tmux-backed local worker terminals");
```

- [ ] **Step 3: Implement CLI grammar**

`terminal.rs` supports:

```text
heiwa terminal launch <name> --session <id> [--instance main] [--cwd <path>] [--json]
heiwa terminal list [--session <id>] [--json]
heiwa terminal send <session>/<name>/<instance> --text <text> [--keys Enter] [--json]
heiwa terminal read <session>/<name>/<instance> [--scrollback N] [--json]
heiwa terminal close <session>/<name>/<instance> [--json]
heiwa terminal reap-orphans [--json]
```

Default spec for `launch shell`:

```rust
TerminalSpec {
    name: "shell".to_string(),
    command: std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string()),
    args: vec!["-l".to_string()],
    env: vec![],
    cwd_root: Some(std::env::current_dir()?),
    allow_cwd_override: true,
    scrollback: 10000,
}
```

- [ ] **Step 4: Write CLI integration test**

Create `apps/heiwa_shell/tests/terminal.rs`. Use temp `HOME` and `Command::new(env!("CARGO_BIN_EXE_heiwa"))`.

Test flow:

```text
heiwa terminal launch shell --session cli-smoke --instance main --cwd <repo> --json
heiwa terminal send cli-smoke/shell/main --text 'printf cli-ok' --keys Enter --json
heiwa terminal read cli-smoke/shell/main --scrollback 20 --json
heiwa terminal close cli-smoke/shell/main --json
```

Assert:

- launch JSON has `status: "launched"` or `already_running`;
- read JSON contains `cli-ok`;
- close JSON has `status: "closed"`;
- state exists under temp `HOME/.heiwa/state/terminals`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p heiwa-shell terminal`

Expected: PASS.

- [ ] **Step 6: Manual smoke**

Run:

```bash
cargo run -p heiwa-shell -- terminal launch shell --session local-smoke --instance main --cwd /Users/dmcgregsauce/heiwa-universe
cargo run -p heiwa-shell -- terminal send local-smoke/shell/main --text 'printf heiwa-terminal-ok' --keys Enter
cargo run -p heiwa-shell -- terminal read local-smoke/shell/main --scrollback 20
cargo run -p heiwa-shell -- terminal close local-smoke/shell/main
```

Expected read output includes `heiwa-terminal-ok`.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_shell crates/heiwa_terminal
git commit -m "feat: add heiwa terminal CLI over tmux registry"
```

---

### Task 5: Doctor Multiplexer Check

**Files:**

- Modify: `crates/heiwa_install/src/lib.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Test: existing shell/doctor tests or new `apps/heiwa_shell/tests/doctor.rs`

- [ ] **Step 1: Extend `DoctorReport`**

Add fields:

```rust
pub tmux_version: Option<String>,
pub tmux_installed: bool,
```

In `check_installation()`:

```rust
let tmux_version = get_version("tmux", &["-V"]);
tmux_installed: tmux_version.is_some(),
tmux_version,
```

- [ ] **Step 2: Extend `AiOpsReport`**

Add:

```rust
pub tmux_available: bool,
```

Update `is_clean()` to include `tmux_available`.

- [ ] **Step 3: Print doctor output**

In `apps/heiwa_shell/src/main.rs`, print:

```rust
println!("  Tmux:   {}", report.tmux_version.clone().unwrap_or_else(|| "Not found".to_string()));
```

In `AI Ops`, add:

```rust
print_ai_ops_check("tmux local multiplexer", ai_ops.tmux_available);
```

- [ ] **Step 4: Update JSON output**

The existing `doctor --json` should include the new fields through serde automatically.

- [ ] **Step 5: Test**

Run:

```bash
cargo test -p heiwa-install
cargo test -p heiwa-shell doctor
cargo run -p heiwa-shell -- doctor --ai-ops --json
```

Expected JSON contains `runtimes.tmux_installed` and `runtimes.tmux_version`.

- [ ] **Step 6: Commit**

```bash
git add crates/heiwa_install/src/lib.rs apps/heiwa_shell/src/main.rs apps/heiwa_shell/tests
git commit -m "feat: surface tmux multiplexer prerequisite in doctor"
```

---

### Task 6: Final Verification And Hand-Off

**Files:**

- Modify: `oss-lifts/omnigent/README.md` only if implementation facts changed.

- [ ] **Step 1: Run focused verification**

```bash
cargo test -p heiwa-terminal
cargo test -p heiwa-shell terminal
cargo test -p heiwa-install
cargo build -p heiwa-shell
```

- [ ] **Step 2: Run manual smoke with cleanup**

```bash
cargo run -p heiwa-shell -- terminal launch shell --session final-smoke --instance main --cwd /Users/dmcgregsauce/heiwa-universe --json
cargo run -p heiwa-shell -- terminal send final-smoke/shell/main --text 'printf final-ok' --keys Enter --json
cargo run -p heiwa-shell -- terminal read final-smoke/shell/main --scrollback 20 --json
cargo run -p heiwa-shell -- terminal close final-smoke/shell/main --json
```

- [ ] **Step 3: Verify no leaked tmux sessions**

```bash
tmux ls 2>/dev/null | rg 'final-smoke|cli-smoke|local-smoke|heiwa' || true
```

Expected: no Heiwa smoke sessions left.

- [ ] **Step 4: Check git status**

```bash
git status --short
```

Expected: only intended files changed, plus pre-existing user changes untouched.

- [ ] **Step 5: Update lift note**

If implementation diverged from plan, update `oss-lifts/omnigent/README.md` with actual files/commands.

- [ ] **Step 6: Commit**

```bash
git add crates/heiwa_terminal apps/heiwa_shell crates/heiwa_install Cargo.toml oss-lifts/omnigent/README.md
git commit -m "docs: finalize tmux multiplexer lift evidence"
```

---

## Out Of Scope For This Plan

- WebSocket/xterm attach in Heiwa.app.
- Remote/shared attach permissions.
- Provider adapter migration to terminal leases.
- Pause/resume semantics beyond send/read/close.
- STDB mirror of terminal receipts.
- Copying OmniGent Python code.
