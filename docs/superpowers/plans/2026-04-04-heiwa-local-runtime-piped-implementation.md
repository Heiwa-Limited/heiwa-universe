# Heiwa Local Runtime Piped Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the authoritative local-runtime substrate for `heiwa` so provider auth, adapters, routing, sessions, and evidence are real before terminal polish, loops, or remote surfaces.

**Architecture:** Keep Rust as the authority spine, STDB as canonical truth, and Python as a bounded migration surface. Phase A is deliberately headless: canonical domain model, install/doctor, provider accounts, provider adapters, DREX capability routing, and evidence-first execution. Terminal UX, persistent PTY ergonomics, bounded `loop`, and `/code` come later on top of that stable substrate.

**Tech Stack:** Rust 1.93.1, Cargo workspace crates, Tokio, Axum, SpacetimeDB, portable PTY library, ratatui/crossterm-style TUI stack, Python 3.14 compatibility wrappers, TypeScript bindings/tooling, Node 24.14.1, npm workspaces, OS keychain integration, local provider CLIs, Ollama.

**Path Convention:** All file paths in this plan are repo-relative and all commands are assumed to run from the repository root.

**Strategic Frame:** Heiwa is not a browser-first coding app. It is a machine-installed AI runtime with unified provider/auth, DREX routing, and evidence/receipt truth. The first 80/20 is the substrate, not the polished terminal app.

**Non-Goals For This Plan:**
- Do not build `app.heiwa.ltd/code` yet.
- Do not replace every Python package immediately.
- Do not introduce browser- or Railway-only execution as the primary product mode.
- Do not build unconstrained autonomous loops without budgets, receipts, and stop conditions.

---

## Program Pipe

`Phase A Foundation | Phase B Terminal Productization | Phase C Compounding Workflows | Phase D Remote Surfaces`

Each phase must land as a working, testable slice before the next becomes authoritative.

## Active Execution Boundary

Only **Phase A: Foundation** should be executed now.

Phase A contains the true 80/20:
- repo and crate spine
- install/doctor baseline
- provider account and auth normalization
- provider adapter contract and initial adapters
- canonical device/provider/session/task/run records
- DREX capability routing
- evidence-first execution

Do **not** start Phase B, C, or D until Phase A is complete and verified.

## Phase Map

### Phase A: Foundation

- Task 1: Establish the Rust Product Workspace
- Task 2: Build `heiwa install` and `heiwa doctor`
- Task 3: Build the Provider Account and Auth Plane
- Task 6: Build the Provider Adapter Layer
- Task 7: Add Device-Aware DREX Capability Routing

### Phase B: Terminal Productization

- Task 4: Add the Persistent Local Session Daemon and PTY Host
- Task 5: Build the REPL, Slash Commands, and `!` Shell Escape
- Task 8: Build Telemetry and the `/telemetry` Pane
- Task 11: Cut Over the Product Surface and Bound Python

### Phase C: Compounding Workflows

- Task 9: Build `heiwa loop` as a bounded runtime workflow
- Task 10: Add the knowledge pipe for research-compounding work

### Phase D: Remote Surfaces

- Deferred in this plan by design
- `app.heiwa.ltd/code`
- org/team surfaces
- remote executive bridge and multi-user control-plane features

---

## Current State To Build From

### Existing authority spine

- `apps/heiwa_core/src/runtime/gateway.rs`
- `apps/heiwa_core/src/runtime/state.rs`
- `apps/heiwa_core/src/stdb/mod.rs`
- `apps/heiwa_core/src/drex/mod.rs`
- `apps/heiwa_core/src/drex/router.rs`
- `apps/heiwa_core/src/drex/scorer.rs`
- `apps/heiwa_core/src/auth.rs`
- `apps/heiwa_hub/spacetimedb/src/lib.rs`

These already own the worker/session/lease/dispatch/evidence authority path and are the correct place to keep canonical runtime state.

### Existing Python CLI migration surface

- `packages/heiwa_cli/heiwa_cli/__main__.py`
- `packages/heiwa_cli/heiwa_cli/repl.py`
- `packages/heiwa_cli/heiwa_cli/commands.py`
- `packages/heiwa_cli/heiwa_cli/status_line.py`
- `packages/heiwa_cli/heiwa_cli/auth.py`
- `packages/heiwa_cli/heiwa_cli/context.py`
- `apps/heiwa_cli/bin/heiwa`
- `apps/heiwa_cli/scripts/setup.sh`
- `apps/heiwa_cli/scripts/bootstrap_env.py`
- `apps/heiwa_cli/scripts/doctor.py`
- `apps/heiwa_cli/scripts/terminal_chat.py`

These provide the right behavioral hints for the product, but they should become wrappers and migration references, not the long-term authority implementation.

### Existing TypeScript/tooling surface

- `package.json`
- `tsconfig.base.json`
- `apps/heiwa_web/package.json`
- `apps/heiwa_web/tsconfig.json`
- `packages/heiwa_bindings/typescript`

The TypeScript workspace is present, but the local runtime product should not depend on the web shell to feel complete.

### Existing product direction docs

- `README.md`
- `docs/standards/runtime-baseline.md`
- `docs/superpowers/plans/2026-04-02-heiwa-foundation-phase-3-plan.md`

---

## Proposed Rust Workspace Layout

Add a first-class Rust product workspace around the current authority spine:

- Create: `apps/heiwa_shell/`
  - binary crate for the user-facing `heiwa` executable
- Create: `crates/heiwa_session/`
  - persistent local session daemon, attach protocol, PTY session model
- Create: `crates/heiwa_repl/`
  - REPL parser, slash command registry, shell passthrough, TUI surface
- Create: `crates/heiwa_provider/`
  - provider account model, auth status, adapter trait, subprocess supervision
- Create: `crates/heiwa_install/`
  - install/doctor/bootstrap logic
- Create: `crates/heiwa_loop/`
  - bounded recursive workflow engine
- Later create: `crates/heiwa_knowledge/`
  - source → raw → wiki → query → output pipe for research/loop memory

Do **not** delete `packages/heiwa_cli` during this plan. Keep it as a compatibility wrapper until the Rust shell reaches feature parity on core flows.

---

## Product Vocabulary

Use these terms consistently:

- **Device**: a machine/runtime host eligible for Heiwa execution
- **Session**: a persistent Heiwa runtime context, usually with one primary PTY
- **Task**: one graph node inside a session
- **Lease**: authorization to execute one task
- **Run**: one execution attempt with metrics, failures, and artifacts
- **Provider Account**: one authenticated provider surface, such as Claude Code or Codex
- **Provider Adapter**: the Heiwa runtime layer that starts, supervises, and normalizes a provider session
- **Loop**: a bounded recursive workflow over the same task/session model

---

## Canonical Runtime Rules

- `heiwa` is the primary product surface.
- `!` always means direct shell passthrough in the active session.
- `/command` always means Heiwa-native action handled by the REPL parser.
- Rust owns authority and policy.
- STDB owns canonical session, provider, routing, run, artifact, and failure truth.
- Python is compatibility only once equivalent Rust behavior exists.
- Every provider interaction must emit normalized events, artifacts, and failures.
- `loop` is budgeted, stoppable, and receipt-backed.

---

## Task 1: Establish the Rust Product Workspace

**Phase:** A — Foundation

**Files:**
- Modify: `Cargo.toml`
- Create: `apps/heiwa_shell/Cargo.toml`
- Create: `apps/heiwa_shell/src/main.rs`
- Create: `crates/heiwa_session/Cargo.toml`
- Create: `crates/heiwa_session/src/lib.rs`
- Create: `crates/heiwa_repl/Cargo.toml`
- Create: `crates/heiwa_repl/src/lib.rs`
- Create: `crates/heiwa_provider/Cargo.toml`
- Create: `crates/heiwa_provider/src/lib.rs`
- Create: `crates/heiwa_install/Cargo.toml`
- Create: `crates/heiwa_install/src/lib.rs`
- Create: `crates/heiwa_loop/Cargo.toml`
- Create: `crates/heiwa_loop/src/lib.rs`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Add a failing workspace smoke test**

Create a small test under `apps/heiwa_shell/tests/` that expects:
- the `heiwa` binary parses `--help`
- the binary can start without web dependencies

- [ ] **Step 2: Run the smoke test to verify it fails**

Run:

```bash
cargo test -p heiwa-shell --test smoke -- --nocapture
```

Expected: FAIL because the crate does not exist yet.

- [ ] **Step 3: Add the new Rust workspace members**

Update `Cargo.toml` to include:
- `apps/heiwa_shell`
- `crates/heiwa_session`
- `crates/heiwa_repl`
- `crates/heiwa_provider`
- `crates/heiwa_install`
- `crates/heiwa_loop`

- [ ] **Step 4: Create minimal crates and a stub `heiwa` binary**

Implement the smallest compileable binary that prints a product-correct help surface:
- `heiwa`
- `heiwa install`
- `heiwa doctor`
- `heiwa auth`
- `heiwa session attach`

- [ ] **Step 5: Re-run the smoke test and full workspace compile**

Run:

```bash
cargo test -p heiwa-shell --test smoke -- --nocapture
TMPDIR=/tmp cargo test --workspace --quiet
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml apps/heiwa_shell crates/heiwa_session crates/heiwa_repl crates/heiwa_provider crates/heiwa_install crates/heiwa_loop
git commit -m "feat: scaffold rust heiwa runtime workspace"
```

## Task 2: Build `heiwa install` and `heiwa doctor`

**Phase:** A — Foundation

**Files:**
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `crates/heiwa_install/src/lib.rs`
- Create: `crates/heiwa_install/src/install.rs`
- Create: `crates/heiwa_install/src/doctor.rs`
- Modify: `apps/heiwa_cli/scripts/setup.sh`
- Modify: `apps/heiwa_cli/scripts/bootstrap_env.py`
- Modify: `packages/heiwa_cli/heiwa_cli/auth.py`
- Test: `crates/heiwa_install/tests/install_doctor.rs`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write failing tests for install/doctor discovery**

Cover:
- Rust/Node/Python runtime detection
- provider CLI discovery for `claude`, `codex`, `gemini`, `openclaw`, `ollama`
- graceful reporting when tools are missing

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p heiwa-install --test install_doctor -- --nocapture
```

Expected: FAIL because discovery/reporting does not exist.

- [ ] **Step 3: Implement `heiwa install`**

`heiwa install` must:
- verify repo/machine prerequisites
- install or guide installation for runtimes and CLIs
- write a local machine manifest under `~/.heiwa/`
- avoid doing network-required provider login automatically

- [ ] **Step 4: Implement `heiwa doctor`**

`heiwa doctor` must report:
- runtime versions
- provider install status
- auth status
- local Ollama status
- key paths
- obvious misconfiguration

- [ ] **Step 5: Wire current shell/bootstrap scripts as compatibility wrappers**

Keep `apps/heiwa_cli/scripts/setup.sh` and `bootstrap_env.py` as migration helpers that call into the new product contract where practical.

- [ ] **Step 6: Re-run tests**

Run:

```bash
cargo test -p heiwa-install --test install_doctor -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_shell/src/main.rs crates/heiwa_install apps/heiwa_cli/scripts/setup.sh apps/heiwa_cli/scripts/bootstrap_env.py packages/heiwa_cli/heiwa_cli/auth.py
git commit -m "feat: add heiwa install and doctor flows"
```

## Task 3: Build the Provider Account and Auth Plane

**Phase:** A — Foundation

**Files:**
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `apps/heiwa_core/src/stdb/mod.rs`
- Modify: `apps/heiwa_core/src/auth.rs`
- Modify: `crates/heiwa_provider/src/lib.rs`
- Create: `crates/heiwa_provider/src/account.rs`
- Create: `crates/heiwa_provider/src/auth.rs`
- Create: `crates/heiwa_provider/tests/provider_auth.rs`
- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- Modify: `packages/heiwa_cli/heiwa_cli/auth.py`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write failing tests for canonical provider accounts**

Cover these provider kinds:
- `oauth_cli`
- `api_key`
- `router_api`
- `local_runtime`
- `custom_profile`

And these fields:
- provider id
- account id
- auth kind
- status
- rate group
- default model
- device binding

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p heiwa-provider --test provider_auth -- --nocapture
```

Expected: FAIL because the account/auth plane is not canonical yet.

- [ ] **Step 3: Add STDB tables for provider accounts and rate state**

In `apps/heiwa_hub/spacetimedb/src/lib.rs`, add the minimal canonical records for:
- provider accounts
- provider entitlements
- provider rate state
- provider model inventory

Do not overbuild billing or team policy here.

- [ ] **Step 4: Implement local auth flows**

Support:
- Claude Code OAuth-backed status/login/logout
- Codex login/status/logout
- Gemini CLI auth surface
- Antigravity/OpenClaw profile auth surface
- local model runtime discovery for Ollama

Use OS keychain or local secure storage for handles and references, not raw plaintext tokens where avoidable.

- [ ] **Step 5: Expose `heiwa auth` and `heiwa providers`**

The local product must provide:
- auth status summary
- provider login
- provider logout
- model inventory by provider

- [ ] **Step 6: Re-run Rust and Python auth tests**

Run:

```bash
cargo test -p heiwa-provider --test provider_auth -- --nocapture
uv run pytest -q packages/heiwa_cli
```

Expected: PASS on the new provider-account/auth coverage.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs apps/heiwa_core/src/stdb/mod.rs apps/heiwa_core/src/auth.rs crates/heiwa_provider packages/heiwa_sdk/heiwa_sdk/spacetimedb.py packages/heiwa_cli/heiwa_cli/auth.py
git commit -m "feat: add canonical provider account and auth plane"
```

## Task 4: Add the Persistent Local Session Daemon and PTY Host

**Phase:** B — Terminal Productization

**Files:**
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `crates/heiwa_session/src/lib.rs`
- Create: `crates/heiwa_session/src/daemon.rs`
- Create: `crates/heiwa_session/src/pty.rs`
- Create: `crates/heiwa_session/src/socket.rs`
- Create: `crates/heiwa_session/tests/session_attach.rs`
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `apps/heiwa_core/src/runtime/state.rs`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write a failing attach/resume test**

Cover:
- start daemon
- attach to existing session
- resume a session after terminal restart

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p heiwa-session --test session_attach -- --nocapture
```

Expected: FAIL because the daemon/PTY surface does not exist.

- [ ] **Step 3: Implement the local daemon and Unix-socket control path**

Support:
- one primary session
- attach/detach
- persistent session id
- local state under `~/.heiwa/`

- [ ] **Step 4: Implement PTY-backed session hosting**

The daemon must host a real PTY session for:
- shell passthrough
- Heiwa REPL interaction
- provider subprocess streaming

- [ ] **Step 5: Persist canonical session state**

Extend STDB and runtime integration so primary local sessions are represented canonically and not only in-memory.

- [ ] **Step 6: Re-run tests**

Run:

```bash
cargo test -p heiwa-session --test session_attach -- --nocapture
TMPDIR=/tmp cargo test -p heiwa-core --quiet
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_shell/src/main.rs crates/heiwa_session apps/heiwa_hub/spacetimedb/src/lib.rs apps/heiwa_core/src/runtime/state.rs
git commit -m "feat: add persistent local heiwa session daemon"
```

## Task 5: Build the REPL, Slash Commands, and `!` Shell Escape

**Phase:** B — Terminal Productization

**Files:**
- Modify: `crates/heiwa_repl/src/lib.rs`
- Create: `crates/heiwa_repl/src/commands.rs`
- Create: `crates/heiwa_repl/src/shell.rs`
- Create: `crates/heiwa_repl/src/status_line.rs`
- Create: `crates/heiwa_repl/src/input.rs`
- Create: `crates/heiwa_repl/tests/command_parse.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Reference only: `packages/heiwa_cli/heiwa_cli/repl.py`
- Reference only: `packages/heiwa_cli/heiwa_cli/commands.py`
- Reference only: `packages/heiwa_cli/heiwa_cli/status_line.py`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write failing parser tests**

Cover:
- plain text routes as task input
- `!git status` routes as direct shell
- `/help`, `/doctor`, `/auth`, `/providers`, `/models`, `/device`, `/telemetry`, `/loop`, `/exit`

- [ ] **Step 2: Run the parser tests to verify they fail**

Run:

```bash
cargo test -p heiwa-repl --test command_parse -- --nocapture
```

Expected: FAIL because the parser and command registry do not exist.

- [ ] **Step 3: Implement the REPL parser and command registry**

Preserve current product semantics:
- `!` = direct shell
- `/command` = Heiwa-native command
- all other text = routed task

- [ ] **Step 4: Implement a statusline/footer telemetry surface**

Include:
- provider
- model
- current route
- current task/run status
- latency
- local/remote state
- turn count

- [ ] **Step 5: Wire `heiwa` to attach to the daemon and open the REPL**

The product must feel complete without the web app.

- [ ] **Step 6: Re-run tests**

Run:

```bash
cargo test -p heiwa-repl --test command_parse -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/heiwa_repl apps/heiwa_shell/src/main.rs
git commit -m "feat: add heiwa repl, commands, and shell escape"
```

## Task 6: Build the Provider Adapter Layer

**Phase:** A — Foundation

**Files:**
- Modify: `crates/heiwa_provider/src/lib.rs`
- Create: `crates/heiwa_provider/src/adapter.rs`
- Create: `crates/heiwa_provider/src/process.rs`
- Create: `crates/heiwa_provider/src/providers/claude_code.rs`
- Create: `crates/heiwa_provider/src/providers/codex.rs`
- Create: `crates/heiwa_provider/src/providers/gemini_cli.rs`
- Create: `crates/heiwa_provider/src/providers/antigravity.rs`
- Create: `crates/heiwa_provider/src/providers/ollama.rs`
- Create: `crates/heiwa_provider/tests/provider_adapters.rs`
- Modify: `apps/heiwa_core/src/runtime/gateway.rs`
- Modify: `apps/heiwa_core/src/runtime/state.rs`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write failing adapter tests**

Cover:
- provider process start
- send input
- collect output
- normalize events
- close session

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p heiwa-provider --test provider_adapters -- --nocapture
```

Expected: FAIL because provider adapters do not exist.

- [ ] **Step 3: Define a single adapter trait**

The trait must support:
- start session
- send input
- read events
- interrupt
- close
- expose capability metadata

- [ ] **Step 4: Implement the initial adapters**

Adapters to land in this task:
- Claude Code
- Codex
- Gemini CLI
- Antigravity
- Ollama

Normalize all outputs into Heiwa events. Do not let provider-specific stdout parsing leak into the rest of the runtime.

- [ ] **Step 5: Emit receipts and artifacts for provider actions**

Every adapter-backed action must connect back to the existing run/artifact/failure path.

- [ ] **Step 6: Re-run tests**

Run:

```bash
cargo test -p heiwa-provider --test provider_adapters -- --nocapture
TMPDIR=/tmp cargo test -p heiwa-core --quiet
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/heiwa_provider apps/heiwa_core/src/runtime/gateway.rs apps/heiwa_core/src/runtime/state.rs
git commit -m "feat: add local provider adapter layer"
```

## Task 7: Add Device-Aware DREX Capability Routing

**Phase:** A — Foundation

**Files:**
- Modify: `apps/heiwa_core/src/drex/mod.rs`
- Modify: `apps/heiwa_core/src/drex/router.rs`
- Modify: `apps/heiwa_core/src/drex/scorer.rs`
- Modify: `apps/heiwa_core/src/drex/policy.rs`
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `crates/heiwa_provider/src/account.rs`
- Create: `apps/heiwa_core/tests/drex_provider_routing.rs`
- Modify: `scripts/tests/test_device_advertising.py`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write failing routing tests**

Cover:
- local-only session
- best compatible provider chosen
- no compatible provider
- privacy veto
- locality veto
- rate-group fallback
- local Ollama scout + stronger provider synthesizer

- [ ] **Step 2: Run the routing tests to verify they fail**

Run:

```bash
cargo test -p heiwa-core --test drex_provider_routing -- --nocapture
```

Expected: FAIL because provider/device routing is not yet canonical.

- [ ] **Step 3: Add canonical device and provider capability records**

Extend STDB with the minimum fields needed to route by:
- device
- provider kind
- model inventory
- locality
- trust
- privacy class
- concurrency
- availability

- [ ] **Step 4: Update DREX scoring**

Make DREX capability-aware across:
- installed provider CLIs
- BYOK/API providers
- router providers
- local model runtimes

- [ ] **Step 5: Attach routing metadata to evidence**

Persist enough routing metadata to explain why a provider/device was chosen.

- [ ] **Step 6: Re-run routing tests**

Run:

```bash
cargo test -p heiwa-core --test drex_provider_routing -- --nocapture
uv run pytest -q scripts/tests/test_device_advertising.py
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_core/src/drex apps/heiwa_hub/spacetimedb/src/lib.rs crates/heiwa_provider/src/account.rs apps/heiwa_core/tests/drex_provider_routing.rs scripts/tests/test_device_advertising.py
git commit -m "feat: add device-aware provider routing"
```

## Task 8: Build Telemetry and the `/telemetry` Pane

**Phase:** B — Terminal Productization

**Files:**
- Modify: `crates/heiwa_repl/src/status_line.rs`
- Create: `crates/heiwa_repl/src/telemetry.rs`
- Create: `crates/heiwa_repl/tests/telemetry_pane.rs`
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `apps/heiwa_core/src/runtime/state.rs`
- Modify: `packages/heiwa_bindings/typescript/index.ts` if bindings exports change

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write failing telemetry tests**

Cover:
- footer rendering
- live counters
- provider/model display
- route state
- current loop state

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p heiwa-repl --test telemetry_pane -- --nocapture
```

Expected: FAIL because the telemetry pane does not exist.

- [ ] **Step 3: Implement the footer and `/telemetry` pane**

Support configurable views for:
- session
- provider
- model
- route
- load
- recent failures
- current task/loop

- [ ] **Step 4: Persist telemetry snapshots minimally**

Only persist what is needed for useful session continuity and debugging.

- [ ] **Step 5: Re-run tests**

Run:

```bash
cargo test -p heiwa-repl --test telemetry_pane -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/heiwa_repl apps/heiwa_hub/spacetimedb/src/lib.rs apps/heiwa_core/src/runtime/state.rs packages/heiwa_bindings/typescript/index.ts
git commit -m "feat: add heiwa telemetry pane"
```

## Task 9: Build `heiwa loop` As A Bounded Runtime Workflow

**Phase:** C — Compounding Workflows

**Files:**
- Modify: `crates/heiwa_loop/src/lib.rs`
- Create: `crates/heiwa_loop/src/engine.rs`
- Create: `crates/heiwa_loop/src/policy.rs`
- Create: `crates/heiwa_loop/src/iteration.rs`
- Create: `crates/heiwa_loop/tests/loop_budget.rs`
- Modify: `apps/heiwa_core/src/runtime/gateway.rs`
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `crates/heiwa_repl/src/commands.rs`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write failing loop tests**

Cover:
- bounded iteration count
- budget stop
- provider/device retargeting
- receipt per iteration
- graceful cancel

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p heiwa-loop --test loop_budget -- --nocapture
```

Expected: FAIL because the loop engine does not exist.

- [ ] **Step 3: Implement the bounded loop engine**

The engine must support:
- plan
- execute
- critique
- continue or stop

Do not ship unconstrained recursive autonomy.

- [ ] **Step 4: Add `/loop` command wiring**

`/loop` must run through the same session/task/receipt path as normal user input.

- [ ] **Step 5: Re-run loop tests**

Run:

```bash
cargo test -p heiwa-loop --test loop_budget -- --nocapture
TMPDIR=/tmp cargo test -p heiwa-core --quiet
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/heiwa_loop apps/heiwa_core/src/runtime/gateway.rs apps/heiwa_hub/spacetimedb/src/lib.rs crates/heiwa_repl/src/commands.rs
git commit -m "feat: add bounded heiwa loop workflow"
```

## Task 10: Add the Knowledge Pipe For Research-Compounding Work

**Phase:** C — Compounding Workflows

**Files:**
- Create: `crates/heiwa_knowledge/Cargo.toml`
- Create: `crates/heiwa_knowledge/src/lib.rs`
- Create: `crates/heiwa_knowledge/src/source.rs`
- Create: `crates/heiwa_knowledge/src/wiki.rs`
- Create: `crates/heiwa_knowledge/src/query.rs`
- Create: `crates/heiwa_knowledge/tests/wiki_flow.rs`
- Modify: `Cargo.toml`
- Modify: `crates/heiwa_loop/src/lib.rs`
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write failing knowledge-pipe tests**

Model the Karpathy-inspired pipe:
- sources
- raw evidence
- wiki knowledge layer
- query
- output

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p heiwa-knowledge --test wiki_flow -- --nocapture
```

Expected: FAIL because the crate and flow do not exist.

- [ ] **Step 3: Implement the minimal knowledge pipe**

Start with:
- source capture
- raw evidence store
- wiki/article synthesis
- query over the synthesized corpus

Keep this local-first and artifact-backed.

- [ ] **Step 4: Wire the loop engine to the knowledge pipe**

The loop should be able to compound knowledge safely:
- read sources
- store evidence
- synthesize wiki nodes
- answer against the resulting corpus

- [ ] **Step 5: Re-run knowledge tests**

Run:

```bash
cargo test -p heiwa-knowledge --test wiki_flow -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/heiwa_knowledge crates/heiwa_loop/src/lib.rs apps/heiwa_hub/spacetimedb/src/lib.rs
git commit -m "feat: add heiwa knowledge pipe"
```

## Task 11: Cut Over the Product Surface and Bound Python

**Phase:** B — Terminal Productization

**Files:**
- Modify: `apps/heiwa_cli/bin/heiwa`
- Modify: `packages/heiwa_cli/heiwa_cli/__main__.py`
- Modify: `packages/heiwa_cli/heiwa_cli/repl.py`
- Modify: `packages/heiwa_cli/heiwa_cli/commands.py`
- Modify: `README.md`
- Modify: `HEIWA.md`
- Modify: `docs/standards/runtime-baseline.md`
- Create or modify: migration notes under `docs/`

**Rollback checkpoint:** If verification fails, revert this task commit and do not continue.

- [ ] **Step 1: Write a failing product smoke test**

Cover:
- `heiwa` launches the Rust shell by default
- Python CLI remains invokable only as a compatibility path
- current install/auth/doctor/repl flows still work

- [ ] **Step 2: Run the smoke test to verify it fails**

Run the narrowest available shell product smoke test for the new binary.

Expected: FAIL because the cutover has not happened yet.

- [ ] **Step 3: Change the default launcher to the Rust shell**

Make `heiwa` invoke the new Rust product surface by default.

- [ ] **Step 4: Bound Python to compatibility roles**

Python may remain for:
- compatibility wrappers
- migration helpers
- specific package lanes not yet migrated

Python must stop being the primary CLI authority.

- [ ] **Step 5: Update docs and product framing**

Make repo docs reflect the truth:
- `heiwa` is the product
- `/code` is deferred
- web is not required for core product use

- [ ] **Step 6: Re-run smoke tests and baseline checks**

Run:

```bash
cargo test --workspace --quiet
uv run pytest -q packages/heiwa_cli
npm run typecheck
bash scripts/check_runtime_baseline.sh
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_cli/bin/heiwa packages/heiwa_cli/heiwa_cli/__main__.py packages/heiwa_cli/heiwa_cli/repl.py packages/heiwa_cli/heiwa_cli/commands.py README.md HEIWA.md docs/standards/runtime-baseline.md docs
git commit -m "feat: cut over to rust heiwa runtime"
```

---

## Phase A Release Gates

Do **not** move to terminal productization until all of these are true:

- the Rust workspace/product crates compile and test cleanly
- `heiwa install` and `heiwa doctor` are real and useful
- provider accounts and auth status are canonical in STDB
- at least one OAuth-backed provider CLI and one local model runtime work through the adapter layer
- DREX can choose across provider/device capability sets
- every tool/model/provider action generates receipts, artifacts, and structured failures
- Python is no longer the only place where provider auth or REPL semantics live

## Later Product Gates

Do **not** call the local runtime product ready until all of these are true:

- `heiwa` launches a persistent local session without the web app
- `!` shell passthrough works in the active session
- `/auth`, `/providers`, `/doctor`, `/telemetry`, and `/loop` work from the REPL
- at least one OAuth-backed provider CLI and one local model runtime work through the adapter layer
- DREX can choose across provider/device capability sets
- every tool/model/provider action generates receipts, artifacts, and structured failures
- Python is no longer the primary CLI authority

## Deferred Work

After Phase A and the later productization phases:

- `app.heiwa.ltd/code` attach surface
- team accounts, billing, and org policy
- Windows/WSL and Linux polish beyond baseline support
- remote boost-node brokering beyond local-first product needs
- desktop wrapper

## Final Verification Checklist

Run all of these before claiming the program slice is complete:

```bash
cargo test --workspace --quiet
uv run pytest -q packages/heiwa_cli apps/heiwa_hub/tests scripts/tests
npm run typecheck
bash scripts/check_runtime_baseline.sh
rg -n "prompt_toolkit|rich|heiwa_cli\\.repl|heiwa_cli\\.commands|heiwa_cli\\.status_line" packages/heiwa_cli apps/heiwa_cli
```

Expected:
- all Rust tests pass
- Python compatibility tests pass
- TypeScript workspace still typechecks
- the grep output should only show bounded compatibility surfaces, not primary-product authority paths

## Immediate Next Move

Start with **Task 1**, then continue through **Task 2**, **Task 3**, **Task 6**, and **Task 7** only.

Do not begin:
- PTY/session daemon UX
- REPL polish
- telemetry pane
- `/loop`
- knowledge pipe
- `/code`

until the foundation phase is complete.
