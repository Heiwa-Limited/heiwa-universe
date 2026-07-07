# Heiwa Terminal v1 Cockpit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform the Heiwa terminal from a line-based REPL into a native, sovereign operator cockpit with deterministic routing and a rich multi-pane TUI.

**Architecture:** Decompose the current `heiwa_shell` monolith into a shared `heiwa_protocol` for state/events and a `heiwa_tui` for rendering, while keeping the Rust runtime as the sole authority.

**Tech Stack:** Rust, Ratatui (TUI), Serde, Anyhow, Chrono, UUID.

---

### Task 1: Create `heiwa_protocol` Crate

**Files:**

- Create: `crates/heiwa_protocol/Cargo.toml`
- Create: `crates/heiwa_protocol/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Define Cargo.toml for `heiwa_protocol`**

```toml
[package]
name = "heiwa-protocol"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.0", features = ["v4"] }
heiwa-bindings = { path = "../../packages/heiwa_bindings/rust" }
```

- [ ] **Step 2: Define core state models in `lib.rs`**

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub transcript: Vec<TranscriptBlock>,
    pub routing: RoutingState,
    pub devices: Vec<DeviceSummary>,
    pub receipts: Vec<RunReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TranscriptBlock {
    User(String),
    Assistant(String),
    Tool(String, String), // name, output
    Evidence(String),     // JSON or summary
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingState {
    pub current_provider: String,
    pub current_model: String,
    pub mode: String, // "Auto", "Manual", "Pinned"
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSummary {
    pub id: String,
    pub hostname: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReceipt {
    pub id: String,
    pub provider: String,
    pub cost: f64,
    pub tokens: u32,
}
```

- [ ] **Step 3: Add to workspace and commit**

```bash
git add crates/heiwa_protocol
git commit -m "feat: initial heiwa_protocol crate for cockpit state"
```

---

### Task 2: Create `heiwa_tui` Crate

**Files:**

- Create: `crates/heiwa_tui/Cargo.toml`
- Create: `crates/heiwa_tui/src/lib.rs`

- [ ] **Step 1: Define Cargo.toml for `heiwa_tui`**

```toml
[package]
name = "heiwa-tui"
version = "0.1.0"
edition = "2021"

[dependencies]
ratatui = "0.26"
crossterm = "0.27"
anyhow = "1.0"
heiwa-protocol = { path = "../heiwa_protocol" }
```

- [ ] **Step 2: Scaffold cockpit layout in `lib.rs`**

```rust
use ratatui::{
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use heiwa_protocol::SessionState;

pub fn render_cockpit(f: &mut Frame, state: &SessionState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Transcript
            Constraint::Length(3), // Composer
            Constraint::Length(1), // Footer
        ])
        .split(f.size());

    // Header
    let header = Paragraph::new(format!("Session: {} | Provider: {}", state.session_id, state.routing.current_provider))
        .block(Block::default().borders(Borders::ALL).title(" Heiwa Cockpit "));
    f.render_widget(header, chunks[0]);

    // Transcript (Placeholder)
    let transcript = Paragraph::new("Transcript will stream here...")
        .block(Block::default().borders(Borders::LEFT | Borders::RIGHT));
    f.render_widget(transcript, chunks[1]);

    // Composer
    let composer = Paragraph::new("> ")
        .block(Block::default().borders(Borders::ALL).title(" Command "));
    f.render_widget(composer, chunks[2]);

    // Footer
    let footer = Paragraph::new(format!("Mode: {} | Cost: $0.00", state.routing.mode));
    f.render_widget(footer, chunks[3]);
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/heiwa_tui
git commit -m "feat: scaffold native cockpit TUI with ratatui"
```

---

### Task 3: Refactor `apps/heiwa_shell` to use Protocol

**Files:**

- Modify: `apps/heiwa_shell/Cargo.toml`
- Modify: `apps/heiwa_shell/src/main.rs`

- [ ] **Step 1: Update Dependencies**

Add `heiwa-protocol` and `heiwa-tui` to `apps/heiwa_shell/Cargo.toml`.

- [ ] **Step 2: Initialize TUI in `main.rs`**

```rust
use heiwa_tui::render_cockpit;
use heiwa_protocol::SessionState;

// In run_repl()
let mut state = SessionState {
    session_id: "default".to_string(),
    transcript: vec![],
    routing: RoutingState {
        current_provider: "none".to_string(),
        current_model: "none".to_string(),
        mode: "Auto".to_string(),
        explanation: None,
    },
    devices: vec![],
    receipts: vec![],
};

// Start crossterm terminal and loop
```

- [ ] **Step 3: Commit**

```bash
git add apps/heiwa_shell/Cargo.toml apps/heiwa_shell/src/main.rs
git commit -m "refactor: wire protocol state and TUI scaffold into heiwa_shell"
```

---

### Task 4: Implement Deterministic Routing Logic

**Files:**

- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `crates/heiwa_protocol/src/lib.rs`

- [ ] **Step 1: Add intent parsing to `heiwa_protocol`**

Implement a function that parses turn instructions (e.g., natural language "use opus") into structured requests.

- [ ] **Step 2: Implement precedence check in `main.rs`**

Implement the logic: Direct Turn > Session Override > Local Profile > Synced Baseline > Device Capability > DREX.

- [ ] **Step 3: Explicitly ban silent fallback**

If the top-precedence target is unavailable, return an error with the reason instead of switching.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: implement deterministic routing precedence and intent parsing"
```

---

### Task 5: Migrate REPL to Event Model

**Files:**

- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `crates/heiwa_loop/src/lib.rs`

- [ ] **Step 1: Replace `println!` with state updates**

In the shell's command handlers, update `state.transcript` instead of printing to stdout.

- [ ] **Step 2: Update `heiwa_loop` to emit protocol events**

Modify the loop execution to return `TranscriptBlock` items.

- [ ] **Step 3: Commit**

```bash
git commit -m "refactor: migrate REPL execution to protocol-driven event model"
```

---

### Task 6: Boot Posture & Fallback

**Files:**

- Modify: `apps/heiwa_shell/src/main.rs`

- [ ] **Step 1: Implement TTY detection**

Use `atty` or similar to detect if `stdout` is a terminal.

- [ ] **Step 2: Implement auto-demote logic**

If not a TTY or `--plain` is passed, use a line-based renderer for protocol events instead of the TUI.

- [ ] **Step 3: Commit**

```bash
git commit -m "feat: add TTY-aware boot logic and plain-mode fallback"
```

---

### Task 7: Final Verification

- [ ] **Step 1: Run full workspace build**

Run: `cargo build --workspace`

- [ ] **Step 2: Run smoke tests**

Run: `cargo test -p heiwa-shell --test smoke`

- [ ] **Step 3: Manual TUI check**

Run: `cargo run -p heiwa-shell` (Verify the layout renders).

- [ ] **Step 4: Manual Plain-mode check**

Run: `cargo run -p heiwa-shell -- --plain` (Verify line-based output).

- [ ] **Step 5: Final Commit**

```bash
git commit -m "chore: finalize terminal v1 cockpit implementation"
```
