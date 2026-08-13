# Heiwa Shape — Implementation Plan

Companion to `~/plans/2026-06-27-heiwa-shape.md` (8 tickets). That doc = _what + why_. This doc = _how_, with code snippets, pattern alignment, and root-pattern refactors.

Prose here is terse on purpose. Code, paths, signatures, lease names, and acceptance criteria are exact — checked against source on 2026-06-27.

Repo truths that reshape the original plan (all confirmed in tree):

- `heiwa doctor` already exists — `apps/heiwa_shell/src/main.rs:284`. TICKET-02 extends, not builds.
- `heiwa app status --json` already exists — `runtime_status()` in `apps/heiwa_shell/src/cmd/app.rs:740`.
- `repo.grep` exists, literal-only — `crates/heiwa_mcp/src/local_tools.rs`. `repo.find` missing. TICKET-01 = add find + upgrade grep.
- Schedule / calendar / mail spine scaffolded — `cmd/schedule.rs`, `cmd/calendar.rs` (`create_hold`), `cmd/mail.rs` (JXA, `metadata-only-no-body`). TICKET-04 + TICKET-06 extend.
- Cockpit composer already streams — `routes/Repl.tsx` POSTs `/api/v1/repl/stream` via `postSse`. TICKET-03 = harden + wire trace/route/receipt, not greenfield.
- Tauri 2 scaffold + `dmg` target exist — `apps/heiwa_app/desktop/src-tauri/`. TICKET-07 = signing + updater + sidecar, not scaffold.

So: most tickets are _extensions of live spine_. Plan reflects that.

---

## Part 0 — Root-pattern refactors (codebase optimization)

These are the large, shared changes. Land them first or alongside ticket-01; every ticket references them. Each is small in code but high-leverage: it removes per-command copy-paste that already shows drift across `cmd/*.rs`.

### R1 — CLI command trait + dispatch table

**Problem.** `cli.rs::try_handle` is a hand-written `match` with one arm per command (`cost`, `life`, `goal`, …). Each `cmd/*.rs` re-implements its own `--json` / `--help` / flag parsing (`has_flag`, `flag_value`, `free_text` duplicated in `app.rs`, `schedule.rs`, others). New tickets (cron, config) add more arms + more copies.

**Fix.** One trait + shared arg helpers in a new module `apps/heiwa_shell/src/cmd/common.rs`.

```rust
// apps/heiwa_shell/src/cmd/common.rs
use anyhow::Result;

/// Shared flag parsing — replaces the per-file copies in app.rs / schedule.rs.
pub fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

pub fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let idx = args.iter().position(|a| a == flag)?;
    args.get(idx + 1).cloned()
}

/// Positional free text = all args that are not flags or flag-values.
pub fn free_text(args: &[String]) -> String {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip { skip = false; continue; }
        if a.starts_with("--") { skip = a != "--json" && a != "--dry-run"; continue; }
        out.push(a.clone());
    }
    out.join(" ").trim().to_string()
}

/// Every top-level `heiwa <verb>` command.
pub trait Command {
    /// Verb as typed: "cron", "config", "app".
    fn name(&self) -> &'static str;
    /// Aliases, e.g. ["automations"] for "auto".
    fn aliases(&self) -> &'static [&'static str] { &[] }
    fn run(&self, args: &[String]) -> Result<()>;
}
```

Keep existing free-function `run`s; the trait wraps them so migration is incremental. `try_handle` becomes a registry lookup:

```rust
// apps/heiwa_shell/src/cli.rs
pub async fn try_handle(args: &[String]) -> Result<bool> {
    let Some(verb) = args.get(1).map(String::as_str) else { return Ok(false) };
    match verb {
        // async commands stay explicit (trait is sync; see note)
        "app" => { cmd::app::run(&args[2..]).await?; Ok(true) }
        "connect" => { cmd::connectors::run(&args[2..]).await?; Ok(true) }
        _ => dispatch_sync(verb, &args[2..]),
    }
}

fn dispatch_sync(verb: &str, rest: &[String]) -> Result<bool> {
    for cmd in registry() {
        if cmd.name() == verb || cmd.aliases().contains(&verb) {
            cmd.run(rest)?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn registry() -> Vec<Box<dyn cmd::common::Command>> {
    vec![
        Box::new(cmd::cost::Cmd), Box::new(cmd::life::Cmd), Box::new(cmd::goal::Cmd),
        Box::new(cmd::compress::Cmd), Box::new(cmd::capabilities::Cmd),
        Box::new(cmd::workers::Cmd), Box::new(cmd::approvals::Cmd),
        Box::new(cmd::auto::Cmd), Box::new(cmd::mail::Cmd),
        Box::new(cmd::schedule::Cmd), Box::new(cmd::calendar::Cmd),
        Box::new(cmd::cron::Cmd),   // TICKET-04
        Box::new(cmd::config::Cmd), // TICKET-08
    ]
}
```

Note: `app` + `connect` are `async`. Don't force the trait async (would pull `async-trait` into the dispatch path for no gain). Keep those two as explicit arms; everything else is sync via the trait. Per-command `Cmd` is a zero-size struct delegating to the existing `run`:

```rust
// at bottom of each cmd/*.rs, e.g. cmd/cost.rs
pub struct Cmd;
impl crate::cmd::common::Command for Cmd {
    fn name(&self) -> &'static str { "cost" }
    fn run(&self, args: &[String]) -> Result<()> { run(args) }
}
```

**Migration is mechanical and safe**: add `common.rs`, delete the local `has_flag`/`flag_value`/`free_text` in `app.rs` + `schedule.rs` and `use crate::cmd::common::*`, add a `Cmd` struct per file. No behavior change. Do this in its own commit before feature work so diffs stay readable.

### R2 — `--json` output envelope

**Problem.** Every command hand-rolls `if has_flag("--json") { println!("{}", json!({...})) } else { println!(...) }`. The JSON shape is ad hoc per command. Cockpit's `Envelope<T>` (see `lib/types.ts`, `unwrap()` in `endpoints.ts`) already expects `{ ok, data, error }`. CLI JSON does not match. Unify.

**Fix.** `cmd/common.rs`:

```rust
use serde::Serialize;
use serde_json::{json, Value};

/// Canonical machine-readable envelope. Matches cockpit `Envelope<T>`.
pub fn emit_json<T: Serialize>(command: &str, data: T) -> Result<()> {
    println!("{}", json!({ "ok": true, "command": command, "data": data }));
    Ok(())
}

pub fn emit_json_err(command: &str, code: &str, message: &str) {
    eprintln!("{}", json!({
        "ok": false, "command": command,
        "error": { "code": code, "message": message }
    }));
}
```

Adopt incrementally. New tickets (cron, config) use it from day one. Backfill `doctor` + `app status` in TICKET-02 since that ticket already touches their JSON and _needs_ it to be the authoritative pre-flight contract. Add a contract test (R2 acceptance) asserting `ok` + `command` + `data` keys exist on every `--json` path touched.

### R3 — ExecutionScope lease constants + grant helper

**Problem.** `main.rs::SessionPins::new` grants leases by string literal:

```rust
grant_tool_lease(&mut scope, "fs.read", RiskClass::HostSafeReadonly);
grant_tool_lease(&mut scope, "repo.grep", RiskClass::HostSafeReadonly);
```

Tool names are also string literals in `local_tools.rs` (`fn name(&self) -> "fs.read"`). Two sources of truth. TICKET-01 adds `repo.find`; easy to forget the lease and silently deny.

**Fix.** Central name constants in `heiwa_mcp` (the crate that owns the tools), re-exported:

```rust
// crates/heiwa_mcp/src/local_tools.rs (top)
pub mod tool_names {
    pub const FS_READ: &str = "fs.read";
    pub const FS_LIST: &str = "fs.list";
    pub const REPO_GREP: &str = "repo.grep";
    pub const REPO_FIND: &str = "repo.find"; // TICKET-01
}
```

Each `Tool::name` returns the const. `SessionPins::new` iterates a table:

```rust
use heiwa_mcp::local_tools::tool_names as tn;
const READONLY_REPO_TOOLS: &[&str] = &[tn::FS_READ, tn::FS_LIST, tn::REPO_GREP, tn::REPO_FIND];
// ...
grant_tool_lease(&mut scope, "shell", RiskClass::HostMutating);
for t in READONLY_REPO_TOOLS { grant_tool_lease(&mut scope, t, RiskClass::HostSafeReadonly); }
```

Now adding a read-only repo tool = one line in `local_repo_registry` + one const + one table entry, and the lease is automatic. `local_repo_registry(scope)` already builds the registry — extend it (TICKET-01) and the names stay in lockstep.

### R4 — Receipt-emit helper for non-provider actions

**Problem.** `heiwa_receipts::Receipt::new` is a 12-arg constructor built for provider token accounting (`tokens_in`, `actual_cost_cad`, …). Tickets 04/05/06 produce _actions_ (cron fire, correction capture, calendar/mail bridge) that must be receipt-backed but have no tokens or cost. Today there's no clean way; risk is people skip receipts for actions.

**Fix.** Thin constructor for zero-cost local actions, in `heiwa_receipts`:

```rust
// crates/heiwa_receipts/src/lib.rs
impl Receipt {
    /// Receipt for a local, non-provider action (cron fire, bridge pull,
    /// correction capture). No tokens, no cost — but chained + auditable.
    pub fn local_action(
        at: i64,
        agent: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Receipt::new(
            at, Env::Local, "heiwa", "none", agent,
            0, 0, 0, 0.0, 0.0, session_id, None,
        )
    }
}
```

(`Env::Local` confirmed against the `Env` enum at `lib.rs:72` — variants are `Local`/`Oauth`/`Api`.) Action commands then do:

```rust
let store = ReceiptStore::open(receipts_db_path())?;
let r = Receipt::local_action(now_unix(), "cron", &session_id);
store.insert(&r)?;
```

This keeps the hash chain intact (`verify_chain`) across action + provider receipts, which is the whole evidence claim.

### R5 — Reuse the NL-parse + approval + hold spine

Not new code — a _rule_. `cmd/schedule.rs` already does: free-text → `parse_via_sdk` (deterministic, `heiwa_sdk/intent/parse_time.py`, no LLM) → `MIN_CONFIDENCE` gate → `--at` escape hatch → `create_hold` + `build_approval_request`. TICKET-04 (cron) and TICKET-06 (calendar import) **must reuse these**, not fork them:

- time parsing → `schedule::parse_via_sdk` (lift to `cmd/common.rs::parse_time` so cron can call it without depending on `schedule`).
- staged write → `calendar::create_hold`.
- approval → `schedule::build_approval_request` (also lift to `common`).

Refactor move: extract `parse_time`, `build_approval_request` from `schedule.rs` into `cmd/common.rs`; `schedule.rs` keeps a re-export. One commit, no behavior change, unblocks 04 + 06.

---

## Part 1 — Ticket implementations

Each ticket: **state** (what's already there), **touch** (files), **build** (snippets), **accept**, **verify**. Refactor deps named as `[R1]…[R5]`.

---

### TICKET-01 — `repo.find` + smarter `repo.grep` (P0, Execution)

**State.** `repo.grep` lives in `crates/heiwa_mcp/src/local_tools.rs`, literal-only (`line.contains(pattern)`), recursive walk skipping `.git|target|node_modules|.venv`, lease-gated via `ensure_lease`, scope-gated via `resolve_existing_path` + `allows_path`. No `repo.find`. FFF lift notes (`oss-lifts/fff/README.md`) pre-scope this exact slice: add `repo.find` read-only, fuzzy path search, return rel paths + scores + scanned/matched counts, respect `max_results`, keep grep intact for now.

**Touch.**

- `crates/heiwa_mcp/src/local_tools.rs` — add `RepoFind`, register, add regex to `RepoGrep`.
- `crates/heiwa_mcp/tests/local_tools.rs` — tests.
- `crates/heiwa_mcp/Cargo.toml` — add `regex`.
- `apps/heiwa_shell/src/main.rs` — lease via `[R3]` table (auto once const added).
- `oss-lifts/fff/README.md` — mark slice landed.

**Build.** Mirror the existing `RepoGrep` struct exactly (clone-of-scope, `new`, `Deserialize+JsonSchema` input, `default_*` fns, `Tool` impl). Add the names const `[R3]`:

```rust
// local_tools.rs — register alongside the others
pub fn local_repo_registry(scope: ExecutionScope) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(FsRead::new(scope.clone()));
    registry.register(FsList::new(scope.clone()));
    registry.register(RepoGrep::new(scope.clone()));
    registry.register(RepoFind::new(scope));   // NEW
    registry
}

#[derive(Clone)]
pub struct RepoFind { scope: ExecutionScope }
impl RepoFind { pub fn new(scope: ExecutionScope) -> Self { Self { scope } } }

#[derive(Deserialize, JsonSchema)]
struct RepoFindInput {
    /// Fuzzy query against repo-relative paths. Empty = list all (bounded).
    #[serde(default)] query: String,
    #[serde(default = "default_dot")] path: String,
    #[serde(default = "default_max_results")] max_results: usize,
}
fn default_max_results() -> usize { 50 }

#[async_trait]
impl Tool for RepoFind {
    fn name(&self) -> &'static str { tool_names::REPO_FIND }
    fn description(&self) -> &'static str {
        "Fuzzy-find files by repo-relative path inside the active execution scope."
    }
    fn input_schema(&self) -> RootSchema { schema_for!(RepoFindInput) }

    async fn call(&self, args: Value) -> Result<Value> {
        ensure_lease(&self.scope, self.name())?;
        let input: RepoFindInput = if args.is_null() {
            RepoFindInput { query: String::new(), path: default_dot(), max_results: default_max_results() }
        } else {
            serde_json::from_value(args).map_err(|source| McpError::InvalidArguments {
                tool: self.name().to_string(), source,
            })?
        };
        let root = resolve_existing_path(&self.scope, &input.path)?;
        let mut scanned = 0usize;
        let mut scored: Vec<(i64, String)> = Vec::new();
        collect_files(&self.scope, &root.absolute, &mut scanned, &mut |rel| {
            match fuzzy_score(&input.query, rel) {
                Some(score) => scored.push((score, rel.to_string())),
                None => {}
            }
        })?;
        // higher score first, then shortest path, then lexical — deterministic
        scored.sort_by(|a, b| b.0.cmp(&a.0)
            .then(a.1.len().cmp(&b.1.len()))
            .then(a.1.cmp(&b.1)));
        let matched = scored.len();
        let results: Vec<Value> = scored.into_iter().take(input.max_results)
            .map(|(score, path)| json!({
                "path": path,
                "name": Path::new(&path).file_name().map(|s| s.to_string_lossy().to_string()),
                "score": score,
            })).collect();
        Ok(json!({
            "path": root.relative, "query": input.query,
            "scanned": scanned, "matched": matched, "results": results,
        }))
    }
}
```

`collect_files` = the same recursive, scope-checked, dir-skipping walk as `grep_path` but yielding repo-relative file paths into a callback (factor the shared walk so grep + find share it — don't copy the `.git|target|node_modules|.venv` skip list twice):

```rust
fn collect_files(
    scope: &ExecutionScope, path: &Path, scanned: &mut usize,
    yield_rel: &mut impl FnMut(&str),
) -> Result<()> {
    let path = fs::canonicalize(path)
        .map_err(|e| McpError::Tool(format!("find resolve failed: {e}")))?;
    if !scope.allows_path(&path) {
        return Err(McpError::PolicyDenied(PolicyDenial::OutsideExecutionScope { path }));
    }
    if path.is_dir() {
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if matches!(name, ".git" | "target" | "node_modules" | ".venv") { return Ok(()); }
        }
        let mut entries = fs::read_dir(&path)
            .map_err(|e| McpError::Tool(format!("find list failed: {e}")))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| McpError::Tool(format!("find list failed: {e}")))?;
        entries.sort_by_key(|e| e.path());
        for e in entries { collect_files(scope, &e.path(), scanned, yield_rel)?; }
    } else if path.is_file() {
        *scanned += 1;
        yield_rel(&relative_to_scope(scope, &path));
    }
    Ok(())
}

/// Subsequence fuzzy match. None = no match. Higher = better (contiguous +
/// word-boundary + early hits rewarded). Keep simple + deterministic; this is
/// the FFF "simple fuzzy scores" slice, not the full fff-core picker.
fn fuzzy_score(query: &str, candidate: &str) -> Option<i64> {
    if query.is_empty() { return Some(0); }
    let (q, c) = (query.to_ascii_lowercase(), candidate.to_ascii_lowercase());
    let cb = c.as_bytes();
    let mut qi = 0usize; let qb = q.as_bytes();
    let mut score = 0i64; let mut last = None::<usize>;
    for (ci, &ch) in cb.iter().enumerate() {
        if qi < qb.len() && ch == qb[qi] {
            score += 1;
            if last == Some(ci.wrapping_sub(1)) { score += 2; }       // contiguous
            if ci == 0 || cb[ci-1] == b'/' || cb[ci-1] == b'_' || cb[ci-1] == b'.' { score += 3; } // boundary
            last = Some(ci); qi += 1;
        }
    }
    (qi == qb.len()).then_some(score)
}
```

`repo.grep` upgrade: add `regex` mode behind an input flag, default literal (back-compat). The literal `line.contains` path stays for empty/invalid patterns; compile a `regex::Regex` when `mode == "regex"`. Add `mode: GrepMode` (`Literal`/`Regex`) to `RepoGrepInput` with `#[serde(default)]` = literal. Keep `max_matches` + match shape (`path`/`line_number`/`line`) identical so cockpit + tests don't break.

```rust
#[derive(Deserialize, JsonSchema, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
enum GrepMode { #[default] Literal, Regex }
// in call(): build a matcher once, then `matcher.is_match(line)` in the loop.
```

**Accept.**

- `repo.find` returns `{scanned, matched, results:[{path,name,score}]}`, paths repo-relative, `results.len() <= max_results`, deterministic order.
- `.git/target/node_modules/.venv` never scanned.
- Out-of-scope `path` → `McpError::PolicyDenied(OutsideExecutionScope)`. Missing lease → `MissingLease`.
- `repo.grep` regex mode matches; literal mode byte-identical to today.
- Lease auto-granted via `[R3]` (no manual edit forgotten).

**Verify.**

```bash
cargo test -p heiwa-mcp --test local_tools -- --nocapture
cargo test -p heiwa-shell --test smoke -- --nocapture   # lease wiring
cargo clippy -p heiwa-mcp -- -D warnings
```

Tests to add: `repo_find_ranks_contiguous_above_scattered`, `repo_find_respects_max_results`, `repo_find_denies_outside_scope`, `repo_find_skips_vendored_dirs`, `repo_grep_regex_mode`, `repo_grep_literal_unchanged`.

---

### TICKET-02 — `heiwa doctor --json` + `app runtime status --json` as authoritative pre-flight (P0, Execution + Evidence)

**State.** Both exist. `doctor` (`main.rs:284`) emits `json!({command, runtimes, identity, providers, heiwa_app, layout, stdb, ai_ops})` — note: bare object, no `ok`/`data` wrapper. `runtime_status` (`app.rs:740`) emits `json!({command:"app runtime status", state, node, …})` — same, unwrapped. `RuntimeStatus::detect()` already aggregates hooks/workers/approvals/mail/local_app. Gap vs ticket: (a) shapes aren't the `[R2]` envelope, so nothing can treat them as a stable contract; (b) no machine "ready/degraded/blocked" verdict — callers must reason over raw fields; (c) no shared pre-flight that other commands call before acting.

**Touch.**

- `apps/heiwa_shell/src/main.rs` — doctor JSON → `[R2]` envelope + `verdict`.
- `apps/heiwa_shell/src/cmd/app.rs` — `runtime_status` JSON → envelope + `verdict`; add `pub fn preflight()`.
- `crates/heiwa_protocol/src/lib.rs` — `Readiness` enum (doctrine_enum) for the verdict.
- `apps/heiwa_shell/tests/smoke.rs` — contract tests.

**Build.** Add a readiness verdict via the existing `doctrine_enum!` macro (consistency with every other state enum in `protocol`):

```rust
// crates/heiwa_protocol/src/lib.rs — alongside the other doctrine_enum! blocks
doctrine_enum! {
    pub enum Readiness {
        Ready    => "ready",     // all checks green
        Degraded => "degraded",  // usable, some checks soft-failing
        Blocked  => "blocked",   // a hard pre-flight check failed
    }
}
```

Compute verdict from already-collected fields, wrap in `[R2]` envelope:

```rust
// main.rs doctor, --json branch
let verdict = if report.missing().is_empty() && identity.is_some() {
    if provider_statuses.iter().any(|p| p.status == "ready") { Readiness::Ready }
    else { Readiness::Degraded }
} else { Readiness::Blocked };

return cmd::common::emit_json("doctor", json!({
    "verdict": verdict.as_str(),
    "runtimes": report, "identity": identity_json, "providers": provider_statuses,
    "heiwa_app": app_probe, "layout": layout, "stdb": stdb, "ai_ops": ai_ops,
}));
```

`preflight()` = the reusable gate other commands call before side effects (cron fire, app update, bridge writes):

```rust
// apps/heiwa_shell/src/cmd/app.rs
#[derive(Debug, Clone, serde::Serialize)]
pub struct Preflight {
    pub verdict: String,           // Readiness::as_str()
    pub local_app_reachable: bool,
    pub approvals_pending: i64,
    pub blockers: Vec<String>,
}

/// Cheap local pre-flight. No provider calls, no network beyond the 200ms
/// localhost probe already in probe_local_app. Other commands call this and
/// refuse to act on `verdict == "blocked"`.
pub fn preflight() -> Preflight {
    let status = RuntimeStatus::detect();
    let mut blockers = Vec::new();
    if !status.local_app.reachable {
        blockers.push("local app unreachable (heiwa app start)".into());
    }
    let pending = status.approvals_summary.get("pending")
        .and_then(Value::as_i64).unwrap_or(0);
    let verdict = if !blockers.is_empty() { "blocked" }
        else if pending > 0 { "degraded" } else { "ready" };
    Preflight { verdict: verdict.into(), local_app_reachable: status.local_app.reachable,
        approvals_pending: pending, blockers }
}
```

`runtime_status` `--json` branch → `emit_json("app runtime status", json!({ "verdict": preflight().verdict, …existing fields }))`.

**Accept.**

- Both `--json` outputs are `{ok:true, command, data:{…}}` with a top-level `data.verdict ∈ {ready,degraded,blocked}`.
- `verdict==blocked` iff a hard check fails (missing runtime, no identity, app unreachable for status).
- `app::preflight()` is `pub`, callable, does no network beyond the existing localhost probe.
- Human (non-`--json`) output unchanged except one added `verdict:` line.

**Verify.**

```bash
heiwa doctor --json | jq -e '.ok and .data.verdict'
heiwa app status --json | jq -e '.data.verdict'
cargo test -p heiwa-shell --test smoke -- --nocapture
```

Add `doctor_json_has_envelope_and_verdict`, `preflight_blocks_when_app_down`.

---

### TICKET-03 — Cockpit composer wired to live session stream (P0, Intake)

**State.** `routes/Repl.tsx` already: composer `textarea` + submit, optimistic user/assistant bubbles, `postSse("/api/v1/repl/stream", {prompt})`, handles `route`/`token`/`done`/`error` SSE events, `TracePills` renders provider/model/mode/privacy/cost/compression. Server side `app.rs::handle_connection` serves `POST /api/v1/repl` and `/api/v1/repl/stream` → `execute_repl_turn`. So the pipe exists. Ticket scope = make it a _daily driver_: (a) abort/cancel in-flight, (b) receipt surfaced post-turn, (c) reconnect/empty-runtime states, (d) wire `+` attach to `repo.find`/`fs.read` context, (e) keyboard + a11y polish.

**Touch.**

- `apps/heiwa_app/clients/cockpit/src/routes/Repl.tsx` — abort handle, receipt row, attach.
- `apps/heiwa_app/clients/cockpit/src/lib/api.ts` — `postSse` already takes `signal`; expose abort to caller (done — just use it).
- `apps/heiwa_app/clients/cockpit/src/lib/types.ts` — `ReplTrace` add `receipt_id?`.
- `apps/heiwa_shell/src/cmd/app.rs` — `/api/v1/repl/stream` emit a final `receipt` SSE event id; `/api/v1/repl/context` (attach) backed by `repo.find`.

**Build.** Abort: `postSse` already accepts `AbortSignal`. Hold a controller in a signal and pass it; cancel on a stop button or new submit.

```tsx
// Repl.tsx — add cancel
const [controller, setController] = createSignal<AbortController | null>(null);

async function submit(): Promise<void> {
  // ...existing optimistic append...
  const ac = new AbortController();
  setController(ac);
  try {
    await postSse("/api/v1/repl/stream", { prompt: text }, (event) => {
      // existing route/token/done/error handling
      if (event.event === "receipt") {
        patchMessage(assistantId, { receipt: event.data as { id: string } });
      }
    }, ac.signal);
  } finally {
    setController(null);
    setBusy(false);
  }
}

function cancel(): void {
  controller()?.abort();
}
```

Stop button swaps with submit while `busy()`:

```tsx
<Show
  when={busy()}
  fallback={<button class="composer-submit" type="submit" disabled={!prompt().trim()}>↑</button>}
>
  <button class="composer-submit stop" type="button" onClick={cancel} aria-label="Stop">■</button>
</Show>;
```

Receipt row: server appends a terminal SSE frame after `done`, carrying the receipt id from the chain so the operator can click through to `Receipts.tsx`. Server side, after the turn:

```rust
// app.rs /api/v1/repl/stream, after execute_repl_turn streams done:
// emit: event: receipt\ndata: {"id": "<receipt id>"}\n\n
```

Attach (`+`): `POST /api/v1/repl/context {query}` → server runs `repo.find` through the **same lease-gated registry** (do not bypass `ExecutionScope`) and returns candidate paths; selected paths get prefixed into the prompt as context refs. This keeps Intake honest — attach uses the same evidence-scoped tools as execution.

Empty-runtime guard: on mount, `v1.health()` (`/status/health`); if unreachable show a connect card instead of the composer:

```tsx
const [online, setOnline] = createSignal(true);
onMount(async () => {
  try {
    await v1.health();
  } catch {
    setOnline(false);
  }
});
// <Show when={online()} fallback={<RuntimeOfflineCard />}>…composer…</Show>
```

**Accept.**

- Stop button aborts the fetch mid-stream; bubble settles, no orphan `streaming:true`.
- After a turn, a receipt id is shown and links to Receipts.
- Runtime-down → offline card, not a dead composer.
- Attach lists files via lease-gated `repo.find`; never reads outside scope.
- Enter submits, Shift+Enter newlines (already), composer has `aria-label` (already).

**Verify.**

```bash
cd apps/heiwa_app/clients/cockpit && npm run typecheck && npm run build
# manual: heiwa app start --port 7474; open cockpit; stream + stop + offline
```

Add a vitest (if/when test harness lands) for the SSE frame parser handling a `receipt` event; until then, typecheck + manual.

---

### TICKET-04 — `heiwa cron` (bounded, approval-gated, receipt-backed) (P0, Execution)

**State.** No `cmd/cron.rs`. But the whole spine cron needs exists: `cmd/schedule.rs` (NL time parse + confidence gate + `--at` + approval request + hold), `cmd/calendar.rs::create_hold`, `cmd/auto.rs` (automations), `cockpit/routes/Crons.tsx` + `endpoints.ts::crons()` (`GET /api/v1/crons`) — UI already expects crons. Receipts via `[R4]`.

**Touch.**

- `apps/heiwa_shell/src/cmd/cron.rs` — new.
- `apps/heiwa_shell/src/cmd/mod.rs` + `cli.rs` — register (via `[R1]`).
- `apps/heiwa_shell/src/cmd/common.rs` — `parse_time`, `build_approval_request` lifted from schedule `[R5]`.
- `crates/heiwa_receipts` — `Receipt::local_action` `[R4]`.
- `apps/heiwa_shell/tests/` — `cron.rs`.

**Build.** Cron entries are local JSON under `~/.heiwa/cron/` (same pattern as calendar holds under `holds_dir()`). Each entry is _bounded_ (max fires, end date), _approval-gated_ (no provider/side-effect action runs without a granted approval), _receipt-backed_ (every fire writes a `local_action` receipt). The runtime daemon (`heiwa app start`) ticks them; the CLI manages them.

```rust
// apps/heiwa_shell/src/cmd/cron.rs
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::cmd::common::{self, has_flag, flag_value, free_text};

#[derive(Debug, Serialize, Deserialize)]
pub struct CronEntry {
    pub id: String,
    pub text: String,          // the user intent
    pub schedule: String,      // cron expr or "every <n><unit>"
    pub next_fire: i64,        // unix secs
    pub max_fires: u32,        // bound — 0 disallowed
    pub fires_done: u32,
    pub ends_at: Option<i64>,  // bound
    pub requires_approval: bool, // default true for any side-effecting action
    pub paused: bool,
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("add")    => add(&args[1..]),
        Some("list")   => list(&args[1..]),
        Some("pause") | Some("resume") => toggle(args),
        Some("rm")     => remove(&args[1..]),
        Some("--help") | None => { print_help(); Ok(()) }
        Some(o) => Err(anyhow!("unknown cron command: {o}")),
    }
}

fn add(args: &[String]) -> Result<()> {
    let text = free_text(args);
    if text.is_empty() {
        return Err(anyhow!("usage: heiwa cron add \"<intent>\" --every 1d --max 10 [--ends YYYY-MM-DD] [--no-approval] [--json]"));
    }
    // bound is MANDATORY — refuse unbounded crons
    let max_fires: u32 = flag_value(args, "--max")
        .ok_or_else(|| anyhow!("--max <n> is required (crons are bounded)"))?
        .parse().map_err(|_| anyhow!("--max must be a positive integer"))?;
    if max_fires == 0 { return Err(anyhow!("--max must be >= 1")); }

    let schedule = flag_value(args, "--every")
        .ok_or_else(|| anyhow!("--every <interval> required, e.g. --every 1d"))?;
    let next_fire = common::parse_time(&format!("in {schedule}"))?  // reuse [R5]
        .get("epoch").and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow!("could not compute next fire"))?;
    let ends_at = flag_value(args, "--ends")
        .map(|s| common::parse_date_epoch(&s)).transpose()?;

    let entry = CronEntry {
        id: format!("cron_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
        text, schedule, next_fire, max_fires, fires_done: 0, ends_at,
        requires_approval: !has_flag(args, "--no-approval"),
        paused: false,
    };
    write_entry(&entry)?;
    if has_flag(args, "--json") {
        common::emit_json("cron add", &entry)
    } else {
        println!("staged cron {} — next fire {}, max {} fires, approval={}",
            entry.id, entry.next_fire, entry.max_fires, entry.requires_approval);
        Ok(())
    }
}
```

The **fire path** (in the runtime daemon, `app.rs` tick loop — add a `tick_crons()` called from the existing keep-awake/heartbeat loop):

```rust
// pseudo — runs inside heiwa app start
fn tick_crons(now: i64, session_id: &str) -> Result<()> {
    for mut entry in load_due_crons(now)? {
        if entry.paused || entry.fires_done >= entry.max_fires { continue; }
        if let Some(end) = entry.ends_at { if now > end { retire(&entry)?; continue; } }

        // bound + approval gate BEFORE any action
        if entry.requires_approval {
            stage_approval(&entry)?;     // schedule::build_approval_request shape [R5]
            // do NOT execute; wait for granted approval, then a later tick acts
        }
        // receipt-back the fire attempt regardless [R4]
        let store = ReceiptStore::open(receipts_db_path())?;
        store.insert(&Receipt::local_action(now, "cron", session_id))?;

        entry.fires_done += 1;
        entry.next_fire = advance(&entry.schedule, now)?;
        write_entry(&entry)?;
    }
    Ok(())
}
```

Cockpit `Crons.tsx` already calls `GET /api/v1/crons`; back it by `load entries → json`. List shape must match cockpit `Cron` type (`lib/types.ts`).

**Accept.**

- `cron add` rejects missing/zero `--max` (no unbounded crons) and missing `--every`.
- Side-effecting crons default `requires_approval=true`; fire stages an approval and does **not** execute until granted.
- Every fire writes a chained `local_action` receipt (`verify_chain` stays valid).
- Entries persist under `~/.heiwa/cron/*.json`; `cron list --json` matches cockpit `Cron`.
- `--ends` past → entry retired, not fired.

**Verify.**

```bash
cargo test -p heiwa-shell --test cron -- --nocapture
heiwa cron add "summarize inbox" --every 1d --max 5 --json | jq -e '.data.requires_approval==true'
heiwa cron add "x" --every 1d   # must error: --max required
```

Tests: `cron_rejects_unbounded`, `cron_defaults_approval_required`, `cron_fire_writes_receipt`, `cron_respects_ends_at`, `cron_list_matches_cockpit_shape`.

---

### TICKET-05 — Operator corrections → tools + eval fixtures (P1, Evidence + Execution)

**State.** No correction-capture path today. `heiwa_loop` (`crates/heiwa_loop/src/lib.rs`: `LoopController`, `LoopConfig`, `LoopStatus`) runs the learning loop against STDB. Receipts crate gives the evidence chain `[R4]`. The Heiwa-shaped move: when an operator corrects an output (in cockpit or CLI), capture it as (a) a typed correction record → STDB belief/evidence, (b) a regenerated eval fixture so the same mistake is tested against next run.

**Touch.**

- `apps/heiwa_shell/src/cmd/` — extend the turn path (where `execute_repl_turn` returns) to accept a `correct` follow-up; or new `cmd/correct.rs`.
- `crates/heiwa_loop/src/lib.rs` — `record_correction()` feeding the loop.
- `crates/heiwa_protocol/src/lib.rs` — `Correction` struct + `CorrectionKind` doctrine_enum.
- `tests/fixtures/corrections/` — generated eval fixtures (new dir; mirror existing test fixture convention).
- cockpit `routes/Repl.tsx` — "correct" affordance on assistant bubbles → `POST /api/v1/correct`.

**Build.** Typed correction (doctrine_enum for the kind, consistent with protocol):

```rust
// crates/heiwa_protocol/src/lib.rs
doctrine_enum! {
    pub enum CorrectionKind {
        Factual   => "factual",    // wrong fact -> contradicts belief
        Routing   => "routing",    // wrong provider/model choice
        Format    => "format",     // right answer wrong shape
        Refusal   => "refusal",    // wrongly refused / wrongly complied
        Scope     => "scope",      // acted outside intent
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    pub id: String,
    pub at: i64,
    pub session_id: String,
    pub receipt_id: String,      // the turn being corrected — chains to evidence [R4]
    pub kind: CorrectionKind,
    pub original: String,        // what Heiwa produced
    pub corrected: String,       // what the operator wanted
    pub note: Option<String>,
}
```

Capture path: a correction (1) writes a `local_action` receipt linking `parent_id = receipt_id` of the corrected turn (chain provenance), (2) emits an EvidenceLink (`EvidenceLinkType::Contradicts` for factual; the enum exists in protocol), (3) writes an eval fixture.

```rust
// crates/heiwa_loop/src/lib.rs
impl LoopController {
    /// Record an operator correction. Pure capture + persistence; the loop
    /// consumes fixtures on its next compile pass. No provider call here.
    pub fn record_correction(&self, c: &Correction) -> Result<(), LoopError> {
        // 1. evidence: chained receipt + contradiction link via STDB
        self.stdb.insert_correction(c)?;            // belief/evidence write
        // 2. fixture: deterministic file the eval harness picks up
        write_eval_fixture(c)?;
        Ok(())
    }
}

/// One fixture = one regression. Named by correction id so reruns are stable.
fn write_eval_fixture(c: &Correction) -> Result<(), LoopError> {
    let dir = std::path::Path::new("tests/fixtures/corrections");
    std::fs::create_dir_all(dir).ok();
    let body = serde_json::json!({
        "kind": c.kind.as_str(),
        "prompt_ref": c.receipt_id,
        "must_not_contain": [c.original],   // factual/format regressions
        "should_resemble": c.corrected,
        "note": c.note,
    });
    std::fs::write(dir.join(format!("{}.json", c.id)),
        serde_json::to_vec_pretty(&body)?)?;
    Ok(())
}
```

Eval harness consumes `tests/fixtures/corrections/*.json` — assert a regenerated answer for `prompt_ref` does not contain any `must_not_contain` string. This is the closed learning loop: correction in → fixture → CI gate → loop compiles the durable belief.

> New surface to add: `LoopController` today is `new(config, stdb, model_tiers)` with no public error enum and no `insert_correction` on `StdbClient`. `record_correction`, the `LoopError` return type, and `StdbClient::insert_correction` are all **new** — define `LoopError` following the crate's existing `Result` convention (mirror `McpError`/`ReceiptError` `thiserror` style), and add the STDB write next to the other `heiwa_stdb` insert methods.

**Accept.**

- A correction writes: a receipt with `parent_id` = corrected turn, an STDB contradiction/evidence link, and a fixture file.
- Fixtures are deterministic (named by id; rerun = same file).
- Eval harness loads every fixture and fails on regression (`must_not_contain` present).
- No provider call in the capture path.

**Verify.**

```bash
cargo test -p heiwa-loop -- --nocapture
ls tests/fixtures/corrections/   # fixture appears after a correction
```

Tests: `correction_writes_chained_receipt`, `correction_emits_fixture`, `fixture_regression_fails_when_original_recurs`.

---

### TICKET-06 — Apple Calendar + Mail local bridge (P1, Intake + Evidence)

**State.** Mail bridge partly built: `cmd/mail.rs` with `POLICY = "metadata-only-no-body"`, a JXA program that pulls Mail.app inbox metadata (no bodies), `bridge_state: "metadata-only-probe"`. Calendar: `cmd/calendar.rs` with `create_hold`, `holds_dir()`. Cockpit `Calendar.tsx`/`Mail.tsx` + `endpoints.ts` (`calendarSummary`, `createHold`, `mailSummary`). Ticket = turn probes into a real read bridge: Calendar via EventKit (read events), Mail via the existing JXA metadata pull, both receipt-backed and policy-gated.

**Touch.**

- `apps/heiwa_shell/src/cmd/calendar.rs` — `import`/`events` subcommand via EventKit (Swift/JXA helper).
- `apps/heiwa_shell/src/cmd/mail.rs` — promote probe → scheduled metadata pull.
- `scripts/` or `apps/heiwa_shell/runtime/` — EventKit helper (Swift binary or JXA `osascript`), same shape as the mail JXA.
- `crates/heiwa_receipts` — `local_action` per bridge pull `[R4]`.
- cockpit `Calendar.tsx`/`Mail.tsx` — render imported events/threads.

**Build.** Match the existing Mail JXA pattern — a sidecar script invoked via `Command`, emitting one JSON row per item on stdout, bodies never touched. Calendar EventKit read (JXA so no extra toolchain; Swift binary if perf matters later):

```javascript
// apps/heiwa_shell/runtime/calendar_pull.jxa  — metadata-only, read-only
// Emits one JSON object per event. No write, no delete. Mirrors mail JXA.
function run() {
  const Cal = Application("Calendar");
  const out = [];
  const horizonDays = 14;
  const now = new Date();
  const until = new Date(now.getTime() + horizonDays * 86400000);
  for (const cal of Cal.calendars()) {
    for (
      const ev of cal.events.whose({
        startDate: { _greaterThan: now },
        _and: [{ startDate: { _lessThan: until } }],
      })()
    ) {
      out.push(JSON.stringify({
        cal: cal.name(),
        title: ev.summary(),
        start: ev.startDate().toISOString(),
        end: ev.endDate().toISOString(),
        all_day: ev.alldayEvent(),
        location: ev.location() || null,
      }));
    }
  }
  return out.join("\n");
}
```

Rust side mirrors `mail.rs::run` structure — invoke, parse rows, receipt the pull, return summary:

```rust
// cmd/calendar.rs — new subcommand
fn import(args: &[String]) -> Result<()> {
    let script = runtime_script("calendar_pull.jxa")?;
    let out = Command::new("osascript").arg("-l").arg("JavaScript").arg(&script)
        .output().map_err(|e| anyhow!("calendar bridge failed: {e}"))?;
    if !out.status.success() {
        return Err(anyhow!("calendar bridge exited {}: {}",
            out.status, String::from_utf8_lossy(&out.stderr)));
    }
    let events: Vec<Value> = String::from_utf8_lossy(&out.stdout).lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    // receipt-back the read [R4]
    let store = ReceiptStore::open(receipts_db_path())?;
    store.insert(&Receipt::local_action(now_unix(), "calendar-bridge", &session_id()))?;
    if has_flag(args, "--json") {
        common::emit_json("calendar import", json!({
            "policy": "read-only-metadata", "count": events.len(), "events": events,
        }))
    } else {
        println!("imported {} events (read-only, receipt-backed)", events.len());
        Ok(())
    }
}
```

Policy: declare `read-only-metadata` for calendar, keep `metadata-only-no-body` for mail. Both bridges **read only** — no event create/delete, no mail send. (Calendar _holds_ are a separate, already-gated write via `create_hold` + approval; the bridge import never writes to Calendar.app.) Schedule periodic pulls via TICKET-04 cron (`heiwa cron add "pull calendar" --every 1h --max 24 --no-approval` — read-only so no approval needed).

**Accept.**

- `heiwa calendar import` reads events via EventKit/JXA, never writes/deletes; `--json` lists events + `policy:"read-only-metadata"`.
- Mail pull stays bodies-never-read (`metadata-only-no-body`).
- Each pull writes a `local_action` receipt.
- Cockpit Calendar/Mail render the imported data.
- Bridge failure (no permission) → clean error, no panic.

**Verify.**

```bash
cargo test -p heiwa-shell --test calendar -- --nocapture
heiwa calendar import --json | jq -e '.data.policy=="read-only-metadata"'
# macOS: first run triggers TCC calendar-access prompt; document in runbook
```

Tests: `calendar_import_is_read_only`, `calendar_pull_parses_rows`, `mail_policy_unchanged`, `bridge_failure_is_clean_error`.

---

### TICKET-07 — macOS Tauri 2 scaffold + signed .dmg + auto-update (P1, Execution)

**State.** Scaffold exists: `apps/heiwa_app/desktop/src-tauri/` (`Cargo.toml` tauri 2.8.5, `lib.rs`/`main.rs`/`proxy.rs`), `tauri.conf.json` with `targets:["app","dmg"]`, but `signingIdentity:null`, `hardenedRuntime:false`, no updater plugin, no notarization. Ticket = signing + hardened runtime + notarization + auto-update (the original "scaffold" is largely done).

**Touch.**

- `apps/heiwa_app/desktop/src-tauri/tauri.conf.json` — signing, hardened runtime, entitlements, updater config.
- `apps/heiwa_app/desktop/src-tauri/Cargo.toml` — `tauri-plugin-updater`.
- `apps/heiwa_app/desktop/src-tauri/src/lib.rs` — register updater plugin.
- `apps/heiwa_app/desktop/src-tauri/entitlements.plist` — new.
- `.github/workflows/` — signed build + notarize + release-asset publish (ties to TICKET-02 `app update --source github`).
- `apps/heiwa_app/desktop/src-tauri/capabilities/` — updater capability.

**Build.** Sign + harden + updater in `tauri.conf.json`:

```jsonc
"bundle": {
  "active": true,
  "targets": ["app", "dmg"],
  "macOS": {
    "minimumSystemVersion": "14.0",
    "signingIdentity": "Developer ID Application: Heiwa Limited (TEAMID)",
    "hardenedRuntime": true,
    "entitlements": "entitlements.plist",
    "providerShortName": "TEAMID"
  }
},
"plugins": {
  "updater": {
    "active": true,
    "endpoints": ["https://github.com/Heiwa-Limited/heiwa-universe/releases/latest/download/latest.json"],
    "dialog": true,
    "pubkey": "<minisign public key>"
  }
}
```

Entitlements — minimal, hardened-runtime-compatible (the app shells out to the `heiwa` sidecar + talks localhost):

```xml
<!-- apps/heiwa_app/desktop/src-tauri/entitlements.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>com.apple.security.cs.allow-jit</key><true/>
  <key>com.apple.security.cs.allow-unsigned-executable-memory</key><true/>
  <key>com.apple.security.network.client</key><true/>
  <key>com.apple.security.automation.apple-events</key><true/> <!-- Calendar/Mail JXA, TICKET-06 -->
</dict></plist>
```

Updater plugin registration:

```rust
// apps/heiwa_app/desktop/src-tauri/src/lib.rs
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        // ...existing setup, proxy, windows...
        .run(tauri::generate_context!())
        .expect("error while running heiwa desktop");
}
```

CI (notarize + publish `latest.json` the updater + TICKET-02 read):

```yaml
# .github/workflows/desktop-release.yml (sketch)
- run: npm ci && npm run build
  working-directory: apps/heiwa_app/clients/cockpit
- uses: tauri-apps/tauri-action@v0
  env:
    APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
    APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
    APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
    APPLE_ID: ${{ secrets.APPLE_ID }}
    APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }} # app-specific pw
    APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
    TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_UPDATER_KEY }}
  with:
    args: --target universal-apple-darwin
    releaseId: ${{ ... }} # publishes dmg + latest.json as release assets
```

This closes the loop with TICKET-02: `heiwa app update --source github` (currently `implemented:false`, blocker = "release asset verification") becomes implementable once `latest.json` + signed assets exist.

**Accept.**

- `.dmg` is signed (`Developer ID Application`), hardened-runtime on, notarized (stapled).
- `spctl -a -vvv Heiwa.app` → accepted; `codesign --verify --deep --strict` clean.
- Updater plugin checks `latest.json`, verifies minisign signature, prompts.
- Entitlements allow localhost + Apple Events (Calendar/Mail) under hardened runtime.
- CI publishes dmg + `latest.json` to GitHub Releases.

**Verify.**

```bash
cd apps/heiwa_app/desktop && npm run tauri build
codesign --verify --deep --strict --verbose=2 src-tauri/target/.../Heiwa.app
spctl -a -vvv -t install src-tauri/target/.../Heiwa.app
xcrun stapler validate src-tauri/target/.../*.dmg
```

---

### TICKET-08 — `~/.heiwa/config.toml` operator-readable + `heiwa config check/show` (P2, Execution)

**State.** `crates/heiwa_config/src/lib.rs`: `HeiwaPaths::resolve()` → `config_path = ~/.heiwa/config.toml`, `load()` reads `[embedding]` only, merges env (`HEIWA_*`) over file over defaults. No `config` CLI command, no validation, no `show` that reveals the merged/effective config. Operators can't see what's actually in effect (env-vs-file precedence is invisible).

**Touch.**

- `apps/heiwa_shell/src/cmd/config.rs` — new (`check`, `show`).
- `cmd/mod.rs` + `cli.rs` — register via `[R1]`.
- `crates/heiwa_config/src/lib.rs` — `AppConfig::effective()` (serializable view) + `validate()`.

**Build.** Make the merged config observable. Add a serializable effective view + validation in the config crate (keep `load()` intact):

```rust
// crates/heiwa_config/src/lib.rs
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveConfig {
    pub config_path: String,
    pub config_present: bool,
    pub embedding_enabled: bool,
    pub embedding_model: String,
    pub ollama_url: Option<String>,
    pub sqlite_path: String,
    /// Per-field origin: "env" | "file" | "default" — makes precedence visible.
    pub sources: std::collections::BTreeMap<String, String>,
}

impl AppConfig {
    pub fn effective(&self) -> EffectiveConfig {
        EffectiveConfig {
            config_path: self.paths.config_path.display().to_string(),
            config_present: self.paths.config_path.is_file(),
            embedding_enabled: self.embedding.enabled,
            embedding_model: self.embedding.model.clone(),
            ollama_url: self.embedding.ollama_url.clone(),
            sqlite_path: self.embedding.sqlite_path.display().to_string(),
            sources: source_origins(),   // computed during load(); see note
        }
    }

    /// Non-fatal validation: returns problems, doesn't panic.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.embedding.enabled && self.embedding.ollama_url.is_none() {
            problems.push("embedding.enabled=true but no ollama_url (remote runtime?)".into());
        }
        if !self.paths.state_dir.is_dir() {
            problems.push(format!("state dir missing: {}", self.paths.state_dir.display()));
        }
        problems
    }
}
```

To populate `sources`, have `load()` record per-field origin as it resolves each field (it already branches env→file→default; capture the winner). Command:

```rust
// apps/heiwa_shell/src/cmd/config.rs
use anyhow::Result;
use crate::cmd::common::{self, has_flag};

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("show")  => show(args),
        Some("check") => check(args),
        _ => { println!("usage: heiwa config <show|check> [--json]"); Ok(()) }
    }
}

fn show(args: &[String]) -> Result<()> {
    let eff = heiwa_config::load().effective();
    if has_flag(args, "--json") { return common::emit_json("config show", &eff); }
    println!("heiwa config ({})", eff.config_path);
    println!("  present: {}", eff.config_present);
    println!("  embedding.enabled: {} [{}]", eff.embedding_enabled,
        eff.sources.get("embedding_enabled").map(String::as_str).unwrap_or("default"));
    println!("  embedding.model:   {} [{}]", eff.embedding_model,
        eff.sources.get("embedding_model").map(String::as_str).unwrap_or("default"));
    println!("  ollama_url:        {:?}", eff.ollama_url);
    Ok(())
}

fn check(args: &[String]) -> Result<()> {
    let cfg = heiwa_config::load();
    let problems = cfg.validate();
    if has_flag(args, "--json") {
        return common::emit_json("config check", serde_json::json!({
            "ok": problems.is_empty(), "problems": problems,
        }));
    }
    if problems.is_empty() { println!("config OK"); }
    else { for p in &problems { println!("  ✗ {p}"); } }
    Ok(())
}
```

**Accept.**

- `heiwa config show` prints effective merged config + per-field origin (env/file/default).
- `heiwa config check` reports problems (non-zero-ish via JSON `ok:false`), never panics on bad/missing file.
- `--json` for both uses `[R2]` envelope.
- `config.toml` absent → `show` works against defaults, marks `present:false`.

**Verify.**

```bash
cargo test -p heiwa-config -- --nocapture
heiwa config show --json | jq -e '.data.config_path'
HEIWA_EMBED_MODEL=foo heiwa config show --json | jq -e '.data.sources.embedding_model=="env"'
heiwa config check --json | jq -e 'has("ok")'
```

Tests: `effective_reflects_env_override`, `validate_flags_enabled_without_url`, `show_works_without_file`.

---

## Part 2 — Formatting rules + pattern alignment

Repo conventions, checked against tooling config. Match these or CI/review bounces the diff.

### Rust (toolchain pinned `1.95.0`, `clippy` + `rustfmt`)

- Format with `cargo fmt` (rustfmt, default config — no custom `rustfmt.toml`). Run before every commit.
- Lint clean: `cargo clippy --workspace -- -D warnings`. New code must not add warnings.
- Errors: library crates use `thiserror` enums (`McpError`, `ReceiptError`, `LoopError`). Binaries (`heiwa_shell`) use `anyhow::Result`. Don't cross the streams — a new lib crate returns a typed error enum, a `cmd/*.rs` returns `anyhow`.
- Tool / command errors carry a typed reason where the audit trail cares (`PolicyDenial` variants), a string where it doesn't (`McpError::Tool(String)`). Follow the existing split.
- State enums (any "kind"/"status"/"state" stored as a string in STDB) go through the `doctrine_enum!` macro in `heiwa_protocol`. Never hand-write `Display`/`FromStr` for these — `Readiness`, `CorrectionKind`, `GrepMode` (local, non-STDB) all use it or its shape.
- Tool impls: derive `Clone` on the struct, hold `scope: ExecutionScope`, `new(scope)`, input type derives `Deserialize + JsonSchema`, defaults via `#[serde(default = "default_*")]` free fns. Copy `RepoGrep` as the template.
- Every tool `call()` opens with `ensure_lease(&self.scope, self.name())?` then arg parse mapping to `McpError::InvalidArguments`. Non-negotiable order — lease first, parse second.
- Path handling: never touch the filesystem with a raw input path. Always `resolve_existing_path` (canonicalize + `allows_path`) and return repo-relative via `relative_to_scope`. This is the scope-safety invariant; reviewers will look for it.
- Receipts: any action that changes state or reaches outside the process writes a receipt. Provider turns → `Receipt::new`; local actions → `Receipt::local_action` `[R4]`. Keep the chain valid (`verify_chain`).

### TypeScript / cockpit (Biome 2.x, Solid)

- Format + lint with Biome: 2-space indent, double quotes, semicolons always, JSON no trailing commas (see `biome.json`). Run `npx biome check --write` on touched cockpit files.
- It's **Solid, not React**: `createSignal`/`createResource`/`onMount`, `<Show>`/`<For>`, `class=` not `className`, no hooks-rules. Don't import React patterns.
- All server reads go through `lib/endpoints.ts::v1.*` which `unwrap()`s the `Envelope<T>`. New endpoints add a typed method there + a type in `lib/types.ts`. Don't `fetch` inline in a route.
- Streaming uses `postSse` (`lib/api.ts`); it already handles `AbortSignal`, multi-line `data:`, and JSON-or-raw frames. Reuse it; don't write a second SSE parser.
- `typecheck` must pass: `npm run typecheck` (strict TS, `tsc --noEmit`).

### JSON contracts (CLI ↔ cockpit)

- One envelope everywhere: `{ ok: bool, command: string, data?: T, error?: {code,message} }` `[R2]`. CLI `--json` and HTTP `/api/v1/*` both emit it; cockpit `unwrap()` consumes `data`.
- Field naming is `snake_case` in all JSON payloads (matches existing `tokens_in`, `next_fire`, `local_app`). TS types map snake_case keys verbatim — don't camelCase at the boundary.

---

## Part 3 — Sequencing, dependencies, CI gates

### Dependency graph

```
R1 (cmd trait/common)  ─┬─> R2 (json envelope) ─┬─> TICKET-02 (doctor/status verdict)
                        │                       ├─> TICKET-04 (cron)  ── needs R4, R5
                        │                       └─> TICKET-08 (config)
R3 (lease consts) ─────────> TICKET-01 (repo.find/grep)
R4 (receipt helper) ─┬─────> TICKET-04 (cron)
                     ├─────> TICKET-05 (corrections)
                     └─────> TICKET-06 (calendar/mail bridge)
R5 (parse/approval lift) ─┬─> TICKET-04 (cron)
                          └─> TICKET-06 (bridge schedule)
TICKET-02 (preflight) ──────> TICKET-04 (cron fire gate), TICKET-07 (app update)
TICKET-03 (composer) ── independent of R*, but attach feature reuses TICKET-01 repo.find
TICKET-07 (sign/updater) ──> unblocks TICKET-02 `app update --source github` impl
```

### Recommended commit order

1. **R1 + R3** — pure refactor, no behavior change. Land first; small reviewable diffs. (`cmd/common.rs`, lease consts.)
2. **R2 + R4 + R5** — shared helpers. Also low-risk.
3. **TICKET-01** — highest-leverage feature, depends only on R3. Visible capability jump.
4. **TICKET-02** — depends on R2; gives every later side-effecting ticket a pre-flight gate.
5. **TICKET-03** — parallelizable (frontend-heavy); attach feature waits on 01.
6. **TICKET-04** — depends on R4, R5, 02. First scheduling primitive.
7. **TICKET-06** — depends on R4, R5; reuses 04 for periodic pulls.
8. **TICKET-05** — depends on R4; loop integration.
9. **TICKET-07** — independent track; unblocks 02's github update path.
10. **TICKET-08** — P2, depends on R1/R2; do anytime after step 2.

This matches the original recommended start order (01 → 02 → 03 → 06) once the R-refactors precede them.

### CI gates (every PR)

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo build --workspace
cargo test -p heiwa-mcp --test local_tools -- --nocapture   # TICKET-01
cargo test -p heiwa-shell --test smoke -- --nocapture        # TICKET-02 + lease wiring
cargo test -p heiwa-loop -- --nocapture                       # TICKET-05
cd apps/heiwa_app/clients/cockpit && npm run typecheck && npm run build && npx biome check
```

Per-ticket regression tests are listed in each ticket's **Verify** block; add them to the relevant crate's `tests/` so the gates above pick them up. TICKET-05 fixtures (`tests/fixtures/corrections/*.json`) become a standing regression set — the learning loop's CI proof.

### Open decisions for the operator

- **Cron daemon host**: the `tick_crons` loop lives in `heiwa app start`. Confirm that's the intended scheduler home vs. a separate `heiwa cron daemon`. (Plan assumes inside `app start`, reusing the existing keep-awake/heartbeat loop.)
- **EventKit access**: TICKET-06 first run triggers a macOS TCC prompt. Decide whether to gate behind an explicit `heiwa calendar connect` consent step (recommended) vs. prompting on first `import`.
- **Updater signing key custody**: TICKET-07 minisign `pubkey` ships in the binary; private key is a CI secret. Confirm key rotation policy before first signed release.

## Part 4 — Designing for N (resources, connectors, machines, data, users)

End goal: a self-aware app that harnesses **N resources, connectors, machines, data → for N users**. The discipline (per CLAUDE.md): **design for N, build for 1, widen on evidence.** We do not build the cloud/multi-tenant plane now — we keep new code from baking in single-X assumptions so 1→N is additive, not a rewrite.

### The one invariant that makes 1→N free

**Every action is attributed to a `(principal, device)` and evidenced in STDB.** If that holds, scaling is "add more principals/devices/accounts," not "re-architect." The seams already exist:

- `SessionPrincipal` (`heiwa_protocol` :378) — who.
- `TreasuryScope::{User,Org,Device,ProviderAccount}` (:114) — the N-accounting model is already typed.
- `ExecutionScope` — per-session capability boundary (working_dir, leases, dirs). Already parameterized; never global.
- Receipt → `ReceiptHeader` → STDB mirror — the shared evidence substrate. STDB is backend authority (hard rule), not an operator surface.

Rule for all new code: **take scope/principal as parameters, never read process-global state** (no bare `env::current_dir()` in tool logic, no singletons keyed to "the machine"). T01 already does this (tools hold `ExecutionScope`); hold that line everywhere.

### The N axes — seam, invariant, what we defer

| Axis                        | Seam in repo                                             | Invariant to preserve now                                                                  | Deferred (until evidence)                |
| --------------------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------------------- |
| **N resources** (inference) | `heiwa_provider`, `heiwa_quota`, treasury enums, routing | Route picks among _available provider accounts_ by cost/quota; never hardcode one provider | Cross-machine quota pooling              |
| **N connectors**            | `heiwa_mcp` registry, `connectors/*.connector.json`      | Adding a connector = register a `Tool` + manifest; zero core change                        | Connector marketplace/discovery          |
| **N machines**              | device identity, STDB sync, `RuntimeStatus.node`         | Every receipt/cron/action carries `device_id`; STDB is the join point                      | Distributed scheduling / leader election |
| **N data**                  | `SourceKind`, `heiwa_embed`, evidence links              | Data tagged by source + principal; embeddings keyed, not global                            | Shared cross-user corpora                |
| **N users**                 | `SessionPrincipal`, `TreasuryScope::{User,Org}`          | Every action attributed to a principal; authorize via `ExecutionScope::authorize`          | Multi-tenant auth, web plane, RBAC UI    |

### N-readiness deltas to the tickets (small, additive)

These are _constraints_, not new scope — they keep the P0/P1 work from precluding N:

- **T01 `repo.find`** — already scope-parameterized. Keep it. No global cwd. (Reference design for "take scope, not globals.")
- **T02 `doctor`/`preflight`** — verdict envelope already carries `node`. **Add `device_id`** (from identity) to the `data` block so multi-machine status can aggregate via STDB later. Aggregation itself = deferred.
- **T04 `cron`** — entry struct **must carry `owner_principal` + `device_id`**, and fire-receipts go to the STDB header (already the mirror). This is what later lets N machines share a schedule without double-firing. Distributed lease = deferred; the _fields_ are not.
- **T05 corrections** — `Correction` already carries `session_id` + `receipt_id`; **add `principal`**. Beliefs land in STDB = the shared learning substrate across users by construction.
- **R2 envelope** — already the multi-surface contract (CLI + HTTP + cockpit). It's N-surface ready as-is.
- **R3 lease consts / registry** — registry-driven tools = N-connector ready as-is. Adding a connector never edits dispatch.

### What stays explicitly OUT of near-term build (anti-theater)

Per "honesty over completeness theater" + "cloud/VPS plane deferred until traction": no multi-tenant auth, no web control plane, no distributed scheduler, no cross-machine quota pooling **yet**. We add the _fields and seams_ (device_id, principal, STDB attribution) now because they're cheap; we add the _machinery_ when a real second machine/user/account exists to prove it.

### How this serves "self-aware"

Self-awareness = the app perceiving its own resources, state, and history, then acting. The N-design makes that perception _complete_: doctor/preflight sees every device + provider account; receipts/STDB are the memory across machines and users; routing reasons over all inference. Build order unchanged — perception (T01/T02) → action (T04) → learning (T05) — but each now emits the attribution that lets the loop reason about an N-shaped world instead of a single box.
