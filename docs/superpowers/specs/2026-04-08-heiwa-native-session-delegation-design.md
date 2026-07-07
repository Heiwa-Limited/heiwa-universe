# Heiwa Native Session Delegation Design

> **Status:** Draft approved in-session for planning
> **Date:** 2026-04-08
> **Scope:** `heiwa-universe`

## Goal

Add a real native-session execution path to Heiwa so provider-owned agent runtimes can be delegated as agents instead of flattened into `prompt -> text`.

This phase is deliberately narrow:

- `A1`: provider-neutral session substrate
- `A2`: Claude Code pilot

It does **not** yet include:

- `A3` task decomposition
- `C` retrieval/context engine
- `B` Heiwa-native tool loop for local models

## One-Sentence Truth

Heiwa should route and supervise provider-owned agent sessions as scoped task executions: `ProviderAdapter` remains the stateless path, `SessionProvider` handles native agent sessions, local disk is the live execution record, and STDB receives an honest projection through existing task/run/artifact primitives.

## Optimization Doctrine

The target is not “use the biggest model.” The target is `quality + accuracy + efficiency`.

That means:

- local models are the default working tier
- remote providers are escalation surfaces
- each step should use the smallest sufficient surface

Current role doctrine:

| Surface                | Default role                                                          |
| ---------------------- | --------------------------------------------------------------------- |
| `ollama/qwen3.5:4b`    | cheap classification, step shaping, summaries, routing prep           |
| `ollama/qwen3.5:9b`    | local coding, diff inspection, bounded verification                   |
| `ollama/gemma4:latest` | local general chat, explanation, rewriting, operator-facing summaries |
| `google-gemini-cli`    | hard reasoning, broad context synthesis, cheap remote escalation      |
| `claude-code`          | highest-value native coding session surface for scoped write tasks    |
| `codex`                | secondary native coding/review surface and alternate execution path   |

End-state rule:

1. local surfaces classify and prepare work
2. local surfaces handle any task that stays within acceptable quality/risk
3. remote surfaces are selected only for capability gaps, accuracy gaps, or native tool leverage
4. local surfaces should post-process, verify, and compress results when feasible

## Why This Exists

Current code still collapses rich provider runtimes into one-shot inference:

- [`crates/heiwa_provider/src/adapter.rs`](../../../crates/heiwa_provider/src/adapter.rs) only defines `ProviderAdapter`.
- [`crates/heiwa_provider/src/providers/claude_code.rs`](../../../crates/heiwa_provider/src/providers/claude_code.rs) wraps Claude Code as `claude -p ... --output-format stream-json`.
- [`crates/heiwa_loop/src/lib.rs`](../../../crates/heiwa_loop/src/lib.rs) sends one objective per turn and ignores `StreamEvent::ToolUse`.
- [`apps/heiwa_hub/orchestrator.py`](../../../apps/heiwa_hub/orchestrator.py) is a tmux spawner, not a delegated execution harness.

That wastes the strongest surfaces already available on this machine.

## Reality Check: Verified CLI Surfaces On This Machine

These capabilities were verified locally on 2026-04-08 via `--help`, not inferred from memory.

| Surface             | Headless     | Structured output           | Resume/fork                                                      | Native tool controls                                                         | Notes                          |
| ------------------- | ------------ | --------------------------- | ---------------------------------------------------------------- | ---------------------------------------------------------------------------- | ------------------------------ |
| `claude-code`       | yes (`-p`)   | yes (`json`, `stream-json`) | yes (`--resume`, `--continue`, `--session-id`, `--fork-session`) | yes (`--allowedTools`, `--add-dir`, `--permission-mode`, `--max-budget-usd`) | richest first pilot            |
| `codex`             | yes (`exec`) | yes (`--json`)              | yes (`resume`, `exec resume`, `fork`)                            | sandbox/approval controls, no explicit tool allowlist                        | better than earlier assumption |
| `google-gemini-cli` | yes (`-p`)   | yes (`json`, `stream-json`) | yes (`--resume`)                                                 | approval mode, include-directories, policy files                             | better than earlier assumption |
| `ollama`            | stateless    | HTTP/CLI                    | none                                                             | none                                                                         | remains `ProviderAdapter` only |

Two implications are locked:

1. Heiwa must model execution-surface capabilities explicitly.
2. Heiwa must not hardcode stale assumptions such as “Gemini has no resume” or “Codex has no durable session.”

## Locked Decisions

### 1. Separate stateless inference from native session delegation

Heiwa keeps the existing stateless adapter path and adds a second trait for native sessions.

```rust
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    async fn send(
        &self,
        model: &str,
        messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()>;

    async fn interrupt(&self) -> anyhow::Result<()>;
    fn supported_models(&self) -> Vec<String>;
}

#[async_trait]
pub trait SessionProvider: ProviderAdapter {
    fn surface_capabilities(&self) -> &ExecutionSurfaceCapabilities;

    async fn start_session(&self, spec: SessionSpec) -> anyhow::Result<SessionHandle>;
    async fn resume_session(
        &self,
        handle: &SessionHandle,
        prompt: Option<String>,
    ) -> anyhow::Result<()>;
    async fn poll_status(&self, handle: &SessionHandle) -> anyhow::Result<SessionStatus>;
    async fn cancel(&self, handle: &SessionHandle) -> anyhow::Result<()>;
    async fn collect_artifacts(
        &self,
        handle: &SessionHandle,
    ) -> anyhow::Result<CollectedArtifacts>;
}
```

This is not a lowest-common-denominator trait. Surfaces without native sessions only implement `ProviderAdapter`.

### 2. Capability discovery lives at the execution-surface layer

`DetectedModel.supports_tools` is not enough. A1 introduces an explicit surface capability record, because headless session behavior is a property of the CLI/runtime surface, not of an individual model tier.

```rust
pub struct ExecutionSurfaceCapabilities {
    pub surface_id: String,                // "claude-code"
    pub provider_family: String,           // "anthropic"
    pub supports_native_session: bool,
    pub supports_resume: bool,
    pub supports_custom_session_id: bool,
    pub supports_stream_json: bool,
    pub supports_json: bool,
    pub supports_interactive_tty: bool,
    pub supports_tool_allowlist: bool,
    pub supports_directory_allowlist: bool,
    pub supports_cost_budget: bool,
    pub supports_turn_budget: bool,
    pub supports_worktree_flag: bool,
}
```

The registry returns a `ProviderHandle`, not a raw trait object:

```rust
pub struct ProviderHandle {
    pub adapter: Arc<dyn ProviderAdapter>,
    pub session: Option<Arc<dyn SessionProvider>>,
    pub surface: ExecutionSurfaceCapabilities,
}
```

This avoids downcast gymnastics while preserving interface segregation.

### 3. A1/A2 only execute already-scoped task steps

This phase does not accept free-form top-level objectives like “fix CI” as session input.

It accepts a pre-scoped `DelegatedTaskSpec`:

```rust
pub struct DelegatedTaskSpec {
    pub task_id: String,
    pub user_id: String,
    pub objective: String,
    pub intent: String,
    pub risk: String,
    pub model_id: String,
    pub workspace: WorkspaceSpec,
    pub output_mode: OutputMode,
    pub budget: SessionBudget,
    pub allowed_tools: Vec<String>,
    pub context_files: Vec<PathBuf>,
    pub expected_artifacts: Vec<String>,
    pub review_policy: ReviewPolicy,
}
```

If the request lacks workspace bounds or mutability policy, A1/A2 rejects it as `requires_decomposition`.

Additional routing rule:

- if a scoped step can be satisfied by local stateless execution at acceptable risk/quality, A1/A2 must not escalate it into a remote native session

### 4. Local disk is the live session store

Session execution must continue to work if STDB is offline.

Canonical live session state for A1/A2 lives under:

`~/.heiwa/sessions/<session_id>/`

Required files:

- `spec.json`
- `status.json`
- `provider_events.jsonl`
- `stdout.log`
- `stderr.log`
- `artifacts.json`
- `usage.json`
- `receipt.json`
- `worktree.json` when applicable
- `diff.patch` when applicable

The local session directory is the truth for active execution. STDB is the projection target.

### 5. STDB uses existing task/run/artifact primitives

A1/A2 does **not** create a parallel “session truth” schema just to mirror local files.

It reuses:

- [`task_dispatches`](../../../apps/heiwa_hub/spacetimedb/src/lib.rs)
- [`artifacts`](../../../apps/heiwa_hub/spacetimedb/src/lib.rs)
- `record_run`
- `update_model_tier_stats`

`task_dispatches` becomes the coarse remote projection for a delegated step.

Required status vocabulary for A1/A2:

- `queued`
- `running`
- `pending_review`
- `complete`
- `failed`
- `cancelled`
- `timed_out`
- `budget_exceeded`
- `rejected`

`loop_sessions` remains loop-specific and is not overloaded for delegated provider sessions.

### 6. Heiwa owns workspace isolation

Provider worktree helpers are not authoritative. Heiwa owns worktree lifecycle because Heiwa must:

- name the worktree deterministically
- know the base commit
- diff the result
- keep acceptance/rejection under operator control

For A2:

- `read_only` sessions may run in-place
- `isolated_write` sessions must run in a Heiwa-created worktree
- `direct_write` is not allowed

Claude’s `--worktree` flag is not used in A2.

### 7. Claude Code is the first session pilot

Claude Code is first because its current CLI surface is the most controllable:

- explicit `--session-id`
- `--resume` and `--continue`
- `--output-format stream-json`
- `--include-hook-events`
- `--allowedTools`
- `--add-dir`
- `--permission-mode`
- `--max-budget-usd`

But one important correction is locked:

- A2 must **not** depend on `--max-turns`, because the installed Claude CLI help on this machine does not advertise that flag.

`SessionBudget.max_interactions` therefore remains a logical Heiwa field, not a guaranteed Claude knob.

## Core Types

### Workspace and budget

```rust
pub enum WorkspaceMode {
    ReadOnly,
    IsolatedWrite,
}

pub struct WorkspaceSpec {
    pub cwd: PathBuf,
    pub mode: WorkspaceMode,
    pub writable_paths: Vec<PathBuf>,
    pub base_revision: Option<String>,
}

pub enum OutputMode {
    Captured,
    Interactive,
}

pub struct SessionBudget {
    pub max_cost_usd: Option<f64>,
    pub timeout_seconds: u32,
    pub max_interactions: Option<u32>,
}

pub enum ReviewPolicy {
    NoReviewRequired,
    RequireReviewOnWrite,
}
```

### Handle and status

```rust
pub struct SessionHandle {
    pub session_id: String,                // Heiwa session id
    pub provider_session_id: String,       // same as session_id for Claude A2
    pub provider_surface: String,
    pub model_id: String,
    pub local_dir: PathBuf,
    pub worktree_dir: Option<PathBuf>,
}

pub enum ExecutionStatus {
    Created,
    Launching,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    BudgetExceeded,
}

pub enum ReviewStatus {
    NotRequired,
    PendingReview,
    Accepted,
    Rejected,
}

pub struct SessionStatus {
    pub execution: ExecutionStatus,
    pub review: ReviewStatus,
    pub summary: Option<String>,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost_usd: f64,
    pub started_at: String,
    pub updated_at: String,
}
```

## Output Modes

### `Captured`

Used for daemon/orchestrator work.

Behavior:

- provider stdout/stderr are piped
- structured events are parsed in real time
- session receipts are complete enough for automation
- this is the default for background tasks

### `Interactive`

Used when the operator wants provider-owned terminal UX.

Behavior:

- provider owns the terminal via inherited stdio
- Heiwa records launch/exit plus post-run artifacts
- full tool-level event fidelity is best-effort, not guaranteed

Rule:

- machine-supervised flows use `Captured`
- operator-watched flows may use `Interactive`

## STDB Projection Model

Each delegated task step projects into STDB like this:

### `task_dispatches`

- one row per delegated step
- `task_id` equals `DelegatedTaskSpec.task_id`
- `assigned_model` stores Heiwa canonical `model_id`
- `sandbox_mode`, `tools_allowed_json`, `context_files_json`, and budgets are populated from `DelegatedTaskSpec`
- status reflects coarse lifecycle only

### `record_run`

- one run receipt per completed session attempt
- `proposal_id = task_id`
- `session_id = heiwa_session_id`
- `mode = "native_session"`
- `model_id = delegated model id`

### `artifacts`

Minimum A2 artifact types:

- `session_metadata`
- `session_event_log`
- `usage_receipt`
- `stdout_log`
- `stderr_log`
- `changed_files`
- `worktree_patch`
- `worktree_diff_stat`

`mission_id` is the delegated `task_id`.

### `update_model_tier_stats`

After completion, Heiwa recomputes rolling stats for the selected model from the latest local session receipts and projects the results into `model_tiers`.

Success rules for A2:

- `complete` + `accepted` or `not_required` => success
- `failed`, `timed_out`, `budget_exceeded`, `rejected` => failure

Escalation rules are also explicit:

- local `qwen3.5` / `gemma4` stay first for routing prep, artifact summarization, and low-risk verification
- `claude-code`, `google-gemini-cli`, and `codex` are only selected when DREX or policy says local quality is insufficient
- A2 may use Claude as the first native-session pilot without turning Claude into the default answer to every coding task

## Claude Code A2

### Launch contract

For captured mode, Claude runs headlessly:

```text
claude -p <objective> \
  --model <provider_model_id> \
  --session-id <session_id> \
  --output-format stream-json \
  --include-hook-events \
  --verbose
```

Optional flags:

- `--allowedTools` from `allowed_tools`
- `--add-dir` for extra writable or readable directories
- `--permission-mode` from policy
- `--max-budget-usd` from `SessionBudget.max_cost_usd`

For interactive mode, Claude runs with inherited stdio and the same `--session-id`.

Resume uses:

```text
claude --resume <session_id>
```

Because Claude accepts `--session-id`, A2 uses the Heiwa UUID as the provider session ID. No separate mapping layer is needed for Claude.

### Parsing rules

The current adapter only extracts `assistant` text and `result` usage.

A2 expands parsing to collect:

- assistant text blocks
- result/usage blocks
- tool or hook lifecycle events when emitted
- error events
- final session metadata

Those normalized events are appended to `provider_events.jsonl` and summarized into artifacts after completion.

### Worktree rule

When `WorkspaceMode::IsolatedWrite` is requested:

1. Heiwa creates the worktree.
2. Claude is launched inside that worktree.
3. Heiwa computes diff artifacts after exit.
4. Session status becomes `pending_review`.

Direct application into the base repo is out of scope for A2.

## Feedback Loop Into Routing

The DREX scorer already uses `last_success_rate` in [`apps/heiwa_core/src/drex/router.rs`](../../../apps/heiwa_core/src/drex/router.rs).

A2 makes that field real by updating it from native-session outcomes.

Local aggregation rule:

- recompute a rolling 20-execution window per model from local session receipts
- update `avg_latency_ms`, `latency_p95_ms`, and `last_success_rate`
- project to STDB when available

This keeps routing honest without making STDB availability a prerequisite for local execution.

## What A1/A2 Explicitly Defers

### A3 Task decomposition

A1/A2 will not pretend broad objectives are already decomposed.

That means:

- no automatic “fix CI” step graph yet
- no automatic local/cloud chaining yet
- no automatic multi-step review pipeline yet

### B Heiwa-native tool execution

For `ollama` and other stateless local surfaces, Heiwa still needs its own tool loop.

That is a separate phase, not a reason to delay provider-native delegation.

### C Context engine

No embedding retrieval or codebase recall is required for A1/A2.

Session delegation must work first on scoped tasks.

## Acceptance Criteria For This Design

The design is correct only if all of the following hold:

- Providers without native sessions remain valid `ProviderAdapter` implementations.
- Session capability is discovered at the surface level, not guessed from model strings.
- Claude sessions can be resumed by the same Heiwa session ID.
- STDB can be offline without breaking active delegated sessions.
- Write-capable sessions do not edit the base repo directly.
- Routing can learn from delegated-session outcomes through `update_model_tier_stats`.
- A1/A2 does not silently depend on decomposition, retrieval, or Heiwa-native tools.
