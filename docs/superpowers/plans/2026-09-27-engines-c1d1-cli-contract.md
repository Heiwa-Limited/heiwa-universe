# C1-d1 CLI Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `heiwa` usable by people and agents alike through one versioned result contract (`heiwa.cli/v1`), stable exit codes, a generated command catalog (`heiwa help --json`), and a resumable Work event stream (`heiwa work watch`).

**Architecture:**
- **Output module.** A bin-private `output` module owns the contract: the envelope, error codes, exit codes, and a single `emit` helper that prints either the envelope or a human view from the same data.
- **Command registry.** A `registry` module replaces the hand-written help. A test scans both dispatchers (`cli.rs` and the `match` in `main.rs`), so the catalog cannot drift again.
- **Retrofits.** Three command groups move to the contract: `work`, `calendar plan`, and `approvals` (rendering only). `work watch` streams NDJSON from the operator journal's existing cursor reader.

**Tech Stack:** Rust (the `heiwa-shell` bin, `anyhow`, `serde_json`), `heiwa_evidence::OperatorJournal`, and Cargo integration tests driving the built binary.

**Spec:** `docs/superpowers/specs/2026-09-27-heiwa-engines-design.md` § Surfaces → CLI.

**Ledger:** Release C1, row C1-d1.

**Boundaries:**
- **Owned elsewhere.** ChatGPT Desktop owns C1-a, which covers authority, containment, and approval decision logic.
  - This plan changes only how `approvals` *renders*.
  - It never touches `decide_request`, `compute_effects`, the `local-cli` operator label, or any approval file.
  - It never touches `cmd/worker.rs` (`heiwa work run`).
- **Rebase order.** If C1-a lands first, rebase this branch and re-run Task 7's tests. Task 7 is ordered last for this reason.
- **Out of scope.** `heiwa ask` and `heiwa run` belong to the answer and execute paths (C1-d2), which need engines.

---

## File Structure

| Path | Change | Responsibility |
| --- | --- | --- |
| `apps/heiwa_shell/src/output.rs` | create | `heiwa.cli/v1` envelope, `ErrorCode`, `CliError`, `emit`, `report_error` |
| `apps/heiwa_shell/src/registry.rs` | create | Command catalog, exit-code table, `heiwa help [--json]`, dispatcher-coverage test |
| `apps/heiwa_shell/src/cmd/args.rs` | create | Shared `has_flag`, `flag_value`, `positionals` for the migrated commands |
| `apps/heiwa_shell/src/main.rs` | modify | Declare modules; route `cli::try_handle` errors; unknown command → exit 2; help → registry |
| `apps/heiwa_shell/src/cmd/mod.rs` | modify | Declare `args` |
| `apps/heiwa_shell/src/cmd/work.rs` | modify | Envelope for list/create/show/recover; new `watch` |
| `apps/heiwa_shell/src/cmd/calendar_plan.rs` | modify | Envelope for diff/stage |
| `apps/heiwa_shell/src/cmd/approvals.rs` | modify | Envelope for list/show/decide rendering; id validation in `show` |
| `apps/heiwa_shell/tests/cli_v1/mod.rs` | create | Test helper: parse one envelope, return `data` or `error` |
| `apps/heiwa_shell/tests/cli_contract.rs` | create | Binary-level contract tests |
| `apps/heiwa_shell/tests/work_support/mod.rs` | modify | Read `work create` through the envelope |
| `apps/heiwa_shell/tests/work_fabric_a1.rs` | modify | Read `work show`/`recover` through the envelope |
| `apps/heiwa_shell/tests/{smoke,schedule,mail_triage,apple_calendar_connector}.rs` | modify | Read `approvals` output through the envelope |
| `scripts/ci_rust_test_group.sh` | modify | Register `cli_contract` in `shell_ops_targets` |
| `docs/design/refs/CLI.md` | modify | Document the result contract and exit code 10 |
| `docs/superpowers/ledgers/2026-08-22-work-fabric-task-ledger.md` | modify | C1-d1 row |

Run every command from the worktree root: `.worktrees/heiwa-engines`.

---

### Task 1: The result contract and error routing

**Files:**
- Create: `apps/heiwa_shell/src/output.rs`
- Modify: `apps/heiwa_shell/src/main.rs:1-3` (module list), `main.rs:209-211` (`cli::try_handle`), `main.rs:864-868` (unknown command)
- Create: `apps/heiwa_shell/tests/cli_v1/mod.rs`, `apps/heiwa_shell/tests/cli_contract.rs`
- Modify: `scripts/ci_rust_test_group.sh:54-64`

- [ ] **Step 1: Write the failing unit tests**

Create `apps/heiwa_shell/src/output.rs` containing only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_success_envelope_carries_schema_data_and_next() {
        let envelope = envelope_ok(json!({"work": []}), &["heiwa work list".to_string()]);
        assert_eq!(envelope["schema"], "heiwa.cli/v1");
        assert_eq!(envelope["ok"], true);
        assert_eq!(envelope["data"]["work"], json!([]));
        assert_eq!(envelope["next"][0], "heiwa work list");
    }

    #[test]
    fn an_error_envelope_carries_code_message_and_hint() {
        let error = CliError::not_found("no Work work-1").with_hint("run `heiwa work list`");
        let envelope = envelope_err(&error);
        assert_eq!(envelope["schema"], "heiwa.cli/v1");
        assert_eq!(envelope["ok"], false);
        assert_eq!(envelope["error"]["code"], "not_found");
        assert_eq!(envelope["error"]["message"], "no Work work-1");
        assert_eq!(envelope["error"]["hint"], "run `heiwa work list`");
    }

    #[test]
    fn exit_codes_follow_the_cli_spec() {
        assert_eq!(ErrorCode::Failure.exit_code(), 1);
        assert_eq!(ErrorCode::Usage.exit_code(), 2);
        assert_eq!(ErrorCode::NotFound.exit_code(), 10);
    }

    #[test]
    fn classification_finds_a_cli_error_under_context() {
        let wrapped = anyhow::Error::new(CliError::usage("bad flag")).context("while parsing");
        assert_eq!(classify(&wrapped).code, ErrorCode::Usage);

        let plain = anyhow::anyhow!("disk full");
        let classified = classify(&plain);
        assert_eq!(classified.code, ErrorCode::Failure);
        assert_eq!(classified.message, "disk full");
    }

    #[test]
    fn json_is_requested_only_by_the_flag() {
        let listed: Vec<String> = ["work", "list", "--json"].map(String::from).to_vec();
        let plain: Vec<String> = ["work", "list"].map(String::from).to_vec();
        assert!(wants_json(&listed));
        assert!(!wants_json(&plain));
    }
}
```

In `apps/heiwa_shell/src/main.rs`, change lines 1-3 to:

```rust
mod cli;
mod cmd;
mod home;
mod output;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p heiwa-shell --bin heiwa output::`

Expected: FAIL to compile with `cannot find function 'envelope_ok'` (and similar).

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `apps/heiwa_shell/src/output.rs`:

```rust
//! The `heiwa` CLI result contract, `heiwa.cli/v1`.
//!
//! One shape for machines and one for people, both from the same data.
//! - With `--json`, a command prints exactly one envelope on stdout.
//! - Without it, the command's human renderer prints to stdout.
//! - An error always prints a readable line on stderr. With `--json` it also
//!   prints an error envelope on stdout, so an agent parses one stream and a
//!   person reads the other.
//!
//! Exit codes follow `docs/design/refs/CLI.md`.

use std::fmt;

use serde_json::{json, Value};

/// Schema of every result envelope.
pub const SCHEMA: &str = "heiwa.cli/v1";
/// Schema of every line in an NDJSON stream such as `heiwa work watch --json`.
pub const STREAM_SCHEMA: &str = "heiwa.cli.stream/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Anything not classified below (exit 1).
    Failure,
    /// A malformed invocation (exit 2).
    Usage,
    /// The named object does not exist (exit 10).
    NotFound,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Failure => "failure",
            ErrorCode::Usage => "usage",
            ErrorCode::NotFound => "not_found",
        }
    }

    pub fn exit_code(self) -> i32 {
        match self {
            ErrorCode::Failure => 1,
            ErrorCode::Usage => 2,
            ErrorCode::NotFound => 10,
        }
    }
}

/// An error a command classifies itself. Anything else is `Failure`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    pub code: ErrorCode,
    pub message: String,
    pub hint: Option<String>,
}

impl CliError {
    pub fn failure(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Failure, message)
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Usage, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// Whether the caller asked for machine output.
pub fn wants_json(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

pub fn envelope_ok(data: Value, next: &[String]) -> Value {
    json!({ "schema": SCHEMA, "ok": true, "data": data, "next": next })
}

pub fn envelope_err(error: &CliError) -> Value {
    json!({
        "schema": SCHEMA,
        "ok": false,
        "error": {
            "code": error.code.as_str(),
            "message": error.message,
            "hint": error.hint,
        },
        "next": [],
    })
}

/// Classify any error. A `CliError` anywhere in the chain keeps its code;
/// anything else is a `Failure` whose message is the full context chain.
pub fn classify(error: &anyhow::Error) -> CliError {
    if let Some(cli) = error.downcast_ref::<CliError>() {
        return cli.clone();
    }
    if let Some(cli) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<CliError>())
    {
        return cli.clone();
    }
    CliError::failure(format!("{error:#}"))
}

/// Print one success: the envelope with `--json`, otherwise the human view.
pub fn emit(json: bool, data: Value, next: &[String], human: impl FnOnce(&Value)) {
    if json {
        println!("{}", envelope_ok(data, next));
    } else {
        human(&data);
    }
}

/// Print an error per the contract and return the process exit code.
pub fn report_error(error: &anyhow::Error, json: bool) -> i32 {
    let classified = classify(error);
    eprintln!("heiwa: {}", classified.message);
    if let Some(hint) = &classified.hint {
        eprintln!("  hint: {hint}");
    }
    if json {
        println!("{}", envelope_err(&classified));
    }
    classified.code.exit_code()
}
```

- [ ] **Step 4: Run the unit tests to verify they pass**

Run: `cargo test -p heiwa-shell --bin heiwa output::`

Expected: PASS, 5 tests.

- [ ] **Step 5: Write the failing binary-level test**

Create `apps/heiwa_shell/tests/cli_v1/mod.rs`:

```rust
//! Reading `heiwa.cli/v1` output in integration tests.
#![allow(dead_code)]

use serde_json::Value;

fn envelope(stdout: &[u8]) -> Value {
    let envelope: Value = serde_json::from_slice(stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not one JSON envelope: {error}: {}",
            String::from_utf8_lossy(stdout)
        )
    });
    assert_eq!(envelope["schema"], "heiwa.cli/v1", "{envelope}");
    envelope
}

/// The `data` of a successful envelope.
pub fn data(stdout: &[u8]) -> Value {
    let envelope = envelope(stdout);
    assert_eq!(envelope["ok"], true, "{envelope}");
    envelope["data"].clone()
}

/// The `error` of a failed envelope.
pub fn error(stdout: &[u8]) -> Value {
    let envelope = envelope(stdout);
    assert_eq!(envelope["ok"], false, "{envelope}");
    envelope["error"].clone()
}

/// The `next` hints of a successful envelope.
pub fn next(stdout: &[u8]) -> Vec<String> {
    let envelope = envelope(stdout);
    envelope["next"]
        .as_array()
        .expect("next is an array")
        .iter()
        .map(|hint| hint.as_str().expect("hint is a string").to_string())
        .collect()
}
```

Create `apps/heiwa_shell/tests/cli_contract.rs`:

```rust
//! The `heiwa.cli/v1` contract, driven through the built binary with an
//! isolated HOME, so no test reads or writes durable operator state.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

mod cli_v1;

fn heiwa(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", home)
        .env("HEIWA_HOME", home.join(".heiwa"))
        .env("HEIWA_DISABLE_KEYCHAIN", "1")
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C")
        .current_dir(home)
        .args(args)
        .output()
        .expect("run heiwa")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn an_unknown_command_is_a_usage_error_on_both_streams() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["definitely-not-a-command", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let error: Value = cli_v1::error(&output.stdout);
    assert_eq!(error["code"], "usage");
    assert!(
        stderr(&output).contains("unknown command: definitely-not-a-command"),
        "{}",
        stderr(&output)
    );
}
```

- [ ] **Step 6: Run it to verify it fails**

Run: `cargo test -p heiwa-shell --test cli_contract`

Expected: FAIL. The exit code is `Some(0)`, because today the unknown-command arm prints help to stdout and exits 0.

- [ ] **Step 7: Route errors through the contract**

In `apps/heiwa_shell/src/main.rs`, replace:

```rust
    if cli::try_handle(&args).await? {
        return Ok(());
    }
```

with:

```rust
    match cli::try_handle(&args).await {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(error) => std::process::exit(output::report_error(&error, output::wants_json(&args))),
    }
```

Replace the catch-all arm at the end of the top-level `match`:

```rust
        _ => {
            println!("Heiwa AI runtime and shell");
            println!("Unknown command: {}", args[1]);
            print_help();
        }
```

with:

```rust
        _ => {
            let error = anyhow::Error::new(
                output::CliError::usage(format!("unknown command: {}", args[1]))
                    .with_hint("run `heiwa help` for the command catalog"),
            );
            std::process::exit(output::report_error(&error, output::wants_json(&args)));
        }
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -p heiwa-shell --test cli_contract && cargo test -p heiwa-shell --bin heiwa output::`

Expected: PASS (1 + 5 tests).

- [ ] **Step 9: Register the new test target with CI**

In `scripts/ci_rust_test_group.sh`, add `cli_contract` to `shell_ops_targets`, after `calendar_sync`:

```bash
shell_ops_targets=(
  apple_mail_connector
  apple_calendar_connector
  calendar_plan_sync
  calendar_sync
  cli_contract
  mail_triage
  schedule
  smoke
  work_fabric_a1
  work_run
)
```

Run: `bash scripts/ci_rust_test_group.sh --check`

Expected: exit 0.

- [ ] **Step 10: Commit**

```bash
git add apps/heiwa_shell/src/output.rs apps/heiwa_shell/src/main.rs apps/heiwa_shell/tests/cli_v1/mod.rs apps/heiwa_shell/tests/cli_contract.rs scripts/ci_rust_test_group.sh
git commit -m "feat(cli): heiwa.cli/v1 result contract; unknown commands exit 2"
```

---

### Task 2: Shared argument helpers

**Files:**
- Create: `apps/heiwa_shell/src/cmd/args.rs`
- Modify: `apps/heiwa_shell/src/cmd/mod.rs`

- [ ] **Step 1: Write the failing tests**

Create `apps/heiwa_shell/src/cmd/args.rs` with only its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn argv(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn flags_and_their_values_are_found() {
        let args = argv(&["work-1", "--since", "c-9", "--json"]);
        assert!(has_flag(&args, "--json"));
        assert_eq!(flag_value(&args, "--since"), Some("c-9"));
        assert_eq!(flag_value(&args, "--missing"), None);
    }

    #[test]
    fn a_flag_is_never_taken_as_a_value() {
        let args = argv(&["--since", "--json"]);
        assert_eq!(flag_value(&args, "--since"), None);
    }

    #[test]
    fn positionals_skip_flags_and_the_values_of_value_flags() {
        let args = argv(&["--since", "c-9", "work-1", "--json", "extra"]);
        assert_eq!(positionals(&args, &["--since"]), vec!["work-1", "extra"]);
    }
}
```

In `apps/heiwa_shell/src/cmd/mod.rs`, add after `pub mod approvals;`:

```rust
pub(crate) mod args;
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p heiwa-shell --bin heiwa cmd::args`

Expected: FAIL to compile with `cannot find function 'has_flag'`.

- [ ] **Step 3: Write the implementation**

Insert above the tests in `apps/heiwa_shell/src/cmd/args.rs`:

```rust
//! Argument helpers shared by command modules that follow `heiwa.cli/v1`.

/// Whether `flag` appears anywhere in `args`.
pub(crate) fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

/// The value after `flag`, unless it is missing or is itself a flag.
pub(crate) fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
}

/// Arguments that are neither flags nor the values of `value_flags`.
pub(crate) fn positionals<'a>(args: &'a [String], value_flags: &[&str]) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut skip_value = false;
    for arg in args {
        if skip_value {
            skip_value = false;
            continue;
        }
        if value_flags.contains(&arg.as_str()) {
            skip_value = true;
            continue;
        }
        if !arg.starts_with("--") {
            found.push(arg.as_str());
        }
    }
    found
}
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p heiwa-shell --bin heiwa cmd::args`

Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_shell/src/cmd/args.rs apps/heiwa_shell/src/cmd/mod.rs
git commit -m "refactor(cli): shared flag and positional helpers"
```

---

### Task 3: The command catalog and `heiwa help --json`

**Files:**
- Create: `apps/heiwa_shell/src/registry.rs`
- Modify: `apps/heiwa_shell/src/main.rs` (module list; help arm near line 858; delete `fn print_help` near line 962)
- Test: `apps/heiwa_shell/tests/cli_contract.rs`

- [ ] **Step 1: Write the failing tests**

Create `apps/heiwa_shell/src/registry.rs` with only its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn catalog_names() -> BTreeSet<&'static str> {
        COMMANDS
            .iter()
            .flat_map(|command| std::iter::once(command.name).chain(command.aliases.iter().copied()))
            .collect()
    }

    /// Names in the `Some("name")` arms of `cli::try_handle`.
    fn cli_dispatch_names() -> BTreeSet<String> {
        include_str!("cli.rs")
            .split("Some(\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .map(str::to_string)
            .collect()
    }

    /// Literal arms of the top-level `match args[1].as_str()` in `main`.
    /// Top-level arms sit at exactly eight spaces; nested arms are deeper.
    fn main_dispatch_names() -> BTreeSet<String> {
        let source = include_str!("main.rs");
        let start = source
            .find("match args[1].as_str() {")
            .expect("main dispatch match");
        let mut names = BTreeSet::new();
        for line in source[start..].lines().skip(1) {
            if line.starts_with("        _ =>") {
                break;
            }
            let Some(arm) = line.strip_prefix("        \"") else {
                continue;
            };
            let head = arm.split("=>").next().unwrap_or("");
            for literal in format!("\"{head}").split('"').skip(1).step_by(2) {
                names.insert(literal.to_string());
            }
        }
        names
    }

    #[test]
    fn every_dispatched_command_is_in_the_catalog() {
        let known = catalog_names();
        let dispatched: BTreeSet<String> = cli_dispatch_names()
            .into_iter()
            .chain(main_dispatch_names())
            .collect();
        assert!(dispatched.len() >= 30, "scanner found too little: {dispatched:?}");
        let missing: Vec<&String> = dispatched
            .iter()
            .filter(|name| !known.contains(name.as_str()))
            .collect();
        assert!(missing.is_empty(), "missing from registry::COMMANDS: {missing:?}");
    }

    #[test]
    fn catalog_names_are_unique() {
        let mut seen = BTreeSet::new();
        for command in COMMANDS {
            for name in std::iter::once(command.name).chain(command.aliases.iter().copied()) {
                assert!(seen.insert(name), "duplicate catalog name {name}");
            }
        }
    }

    #[test]
    fn every_v1_invocation_starts_with_its_command() {
        for command in COMMANDS {
            for invocation in command.v1 {
                assert_eq!(
                    invocation.split(' ').next(),
                    Some(command.name),
                    "{invocation} is listed under {}",
                    command.name
                );
            }
        }
    }

    #[test]
    fn the_exit_code_table_agrees_with_error_codes() {
        use crate::output::ErrorCode;
        for code in [ErrorCode::Failure, ErrorCode::Usage, ErrorCode::NotFound] {
            assert!(
                EXIT_CODES
                    .iter()
                    .any(|(number, name, _)| *number == code.exit_code() && *name == code.as_str()),
                "{code:?} missing from EXIT_CODES"
            );
        }
    }

    #[test]
    fn the_catalog_json_lists_commands_and_exit_codes() {
        let catalog = catalog_json();
        let commands = catalog["commands"].as_array().expect("commands");
        assert_eq!(commands.len(), COMMANDS.len());
        let calendar = commands
            .iter()
            .find(|command| command["name"] == "calendar")
            .expect("calendar is catalogued");
        assert_eq!(calendar["usage"], "heiwa calendar status|sync|hold|plan");
        let codes: Vec<i64> = catalog["exit_codes"]
            .as_array()
            .expect("exit codes")
            .iter()
            .map(|code| code["code"].as_i64().expect("code"))
            .collect();
        assert_eq!(codes, vec![0, 1, 2, 3, 4, 5, 10]);
    }
}
```

In `apps/heiwa_shell/src/main.rs`, add `mod registry;` after `mod output;`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p heiwa-shell --bin heiwa registry::`

Expected: FAIL to compile with `cannot find value 'COMMANDS'`.

- [ ] **Step 3: Write the registry**

Insert above the tests in `apps/heiwa_shell/src/registry.rs`:

```rust
//! The `heiwa` command catalog behind `heiwa help` and `heiwa help --json`.
//!
//! One table describes every top-level command. A test scans both
//! dispatchers (`cli::try_handle` and the `match` in `main`), so a command
//! cannot ship without an entry; that gap is how the hand-written help went
//! stale.
//!
//! `v1` lists the invocations whose output follows the `heiwa.cli` contract
//! today. Everything else still prints its own format until it migrates.

use serde_json::{json, Value};

pub struct CommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub usage: &'static str,
    pub summary: &'static str,
    pub v1: &'static [&'static str],
}

const fn command(name: &'static str, usage: &'static str, summary: &'static str) -> CommandSpec {
    CommandSpec {
        name,
        aliases: &[],
        usage,
        summary,
        v1: &[],
    }
}

pub const COMMANDS: &[CommandSpec] = &[
    command("install", "install [gh:owner/repo[@ref]]", "Bootstrap Heiwa or install a GitHub plugin"),
    command("setup", "setup [--name <name>]", "First-run setup: identity, provider, readiness"),
    command("whoami", "whoami", "Show this installation's local identity"),
    command("login", "login [token]", "Sign in to Heiwa"),
    command("logout", "logout", "Sign out from Heiwa"),
    command("register", "register", "Register the current device"),
    command("devices", "devices", "Show registered devices"),
    command("doctor", "doctor [--ai-ops] [--json]", "Check installation, identity, providers, local app reachability"),
    command("auth", "auth status|add-key|login|logout", "Connected accounts, API keys, and provider CLI login"),
    command("providers", "providers", "List connected accounts and models"),
    command("models", "models", "List all detected models by rate group"),
    command("connect", "connect <connector>", "Connect a provider CLI or life connector"),
    command("ask", "ask <prompt>", "Run one non-interactive turn and print the reply"),
    command("route", "route preview <prompt>", "Preview DREX routing without execution"),
    command("session", "session attach", "Attach to a Heiwa session"),
    command("loop", "loop [turns] <objective>", "Run a bounded execution loop"),
    command("shell", "shell", "Enter interactive mode"),
    command("goal", "goal <subcommand>", "Long-running goals: start, show, step, finish"),
    command("work", "work list|create|show|run|recover", "Durable Work on this installation"),
    command("workspace", "workspace status|prepare", "Repository hold for a Work"),
    command("workers", "workers heartbeat|status", "Worker liveness registry"),
    command("approvals", "approvals list|show|decide", "Review and decide staged actions"),
    command("receipts", "receipts", "Show run receipt status"),
    command("cost", "cost", "Token and cost totals from local receipts"),
    command("compress", "compress [--text|--file] [--json]", "Compress text with a local model"),
    command("calendar", "calendar status|sync|hold|plan", "Calendar lanes, local holds, and plan sync"),
    command("schedule", "schedule <text>", "Turn free text into a staged calendar hold"),
    command("mail", "mail status|accounts", "Mail.app metadata-only bridge probe"),
    command("life", "life status|today|freshness|approvals|import", "Inspect and import life read models"),
    CommandSpec {
        aliases: &["automations"],
        ..command("auto", "auto status|create|tick", "Manage local background automations")
    },
    command("capabilities", "capabilities", "Refresh the local capability inventory"),
    command("mesh", "mesh status|enroll", "Node identity for this machine (no peers yet)"),
    command("app", "app [runtime status]", "Probe local Heiwa.app runtime readiness"),
    CommandSpec {
        aliases: &["--help", "-h"],
        v1: &["help"],
        ..command("help", "help [--json]", "Print this catalog")
    },
    CommandSpec {
        aliases: &["--version", "-V"],
        ..command("version", "version", "Print the heiwa version")
    },
];

/// Exit codes from `docs/design/refs/CLI.md`: (code, name, meaning).
pub const EXIT_CODES: &[(i32, &str, &str)] = &[
    (0, "ok", "success"),
    (1, "failure", "generic failure"),
    (2, "usage", "malformed invocation"),
    (3, "provider_auth", "provider authentication failure"),
    (4, "approval_required", "approval required and not granted"),
    (5, "sandbox_required", "sandbox required and unavailable"),
    (10, "not_found", "the named object does not exist"),
];

pub fn catalog_json() -> Value {
    json!({
        "commands": COMMANDS
            .iter()
            .map(|command| json!({
                "name": command.name,
                "aliases": command.aliases,
                "usage": format!("heiwa {}", command.usage),
                "summary": command.summary,
                "v1": command.v1,
            }))
            .collect::<Vec<_>>(),
        "exit_codes": EXIT_CODES
            .iter()
            .map(|(code, name, meaning)| json!({"code": code, "name": name, "meaning": meaning}))
            .collect::<Vec<_>>(),
        "contract": {
            "result": crate::output::SCHEMA,
            "stream": crate::output::STREAM_SCHEMA,
            "json_flag": "--json prints one result envelope on stdout; diagnostics go to stderr",
        },
    })
}

pub fn print_help() {
    println!("Heiwa — BYOK terminal agent");
    println!();
    println!("Usage: heiwa [COMMAND]");
    println!();
    println!("Commands:");
    for command in COMMANDS {
        println!("  {:<44} {}", command.usage, command.summary);
    }
    println!();
    println!("Machine-readable catalog: heiwa help --json");
}

/// `heiwa help [--json]`.
pub fn run(args: &[String]) {
    crate::output::emit(crate::output::wants_json(args), catalog_json(), &[], |_| {
        print_help()
    });
}
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p heiwa-shell --bin heiwa registry::`

Expected: PASS, 5 tests.

- [ ] **Step 5: Add the failing binary test for `help --json`**

Append to `apps/heiwa_shell/tests/cli_contract.rs`:

```rust
#[test]
fn help_json_is_a_v1_catalog_that_includes_every_command() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["help", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let data = cli_v1::data(&output.stdout);
    let names: Vec<&str> = data["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .map(|command| command["name"].as_str().expect("name"))
        .collect();
    for expected in ["calendar", "connect", "approvals", "work", "version"] {
        assert!(names.contains(&expected), "{expected} missing from {names:?}");
    }
}

#[test]
fn human_help_lists_commands_the_old_help_omitted() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["help"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("calendar status|sync|hold|plan"), "{text}");
    assert!(text.contains("connect <connector>"), "{text}");
}
```

Run: `cargo test -p heiwa-shell --test cli_contract help`

Expected: FAIL, because `help --json` still prints the old text help and stdout is not JSON.

- [ ] **Step 6: Wire help to the registry and delete the old help**

In `apps/heiwa_shell/src/main.rs`, replace the help arm:

```rust
        "--help" | "-h" | "help" => {
            print_help();
        }
```

with:

```rust
        "--help" | "-h" | "help" => registry::run(&args[2..]),
```

Then delete the whole `fn print_help() { ... }` function (it starts `fn print_help() {` near line 962 and ends after `println!("  help                          Print this message");`).

Run: `grep -n 'print_help()' apps/heiwa_shell/src/main.rs`

Expected: no output.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p heiwa-shell --test cli_contract && cargo test -p heiwa-shell --bin heiwa registry::`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/heiwa_shell/src/registry.rs apps/heiwa_shell/src/main.rs apps/heiwa_shell/tests/cli_contract.rs
git commit -m "feat(cli): generated command catalog behind heiwa help [--json]"
```

---

### Task 4: `heiwa work` on the contract

**Files:**
- Modify: `apps/heiwa_shell/src/cmd/work.rs:1-267` (imports, `run`, `list`, `create_command`, `show_command` head, `recover_command`) and `:405-414` (delete the local flag helpers)
- Modify: `apps/heiwa_shell/src/registry.rs` (`work` entry)
- Modify: `apps/heiwa_shell/tests/work_support/mod.rs`, `apps/heiwa_shell/tests/work_fabric_a1.rs`
- Test: `apps/heiwa_shell/tests/cli_contract.rs`

- [ ] **Step 1: Write the failing binary tests**

Append to `apps/heiwa_shell/tests/cli_contract.rs`:

```rust
#[test]
fn work_list_is_an_envelope_that_suggests_creating_work() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["work", "list", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(cli_v1::data(&output.stdout)["work"], serde_json::json!([]));
    assert!(
        cli_v1::next(&output.stdout)[0].starts_with("heiwa work create"),
        "{:?}",
        cli_v1::next(&output.stdout)
    );
}

#[test]
fn showing_unknown_work_is_not_found() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["work", "show", "work-missing", "--json"]);
    assert_eq!(output.status.code(), Some(10), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "not_found");
}

#[test]
fn an_unknown_work_subcommand_is_a_usage_error() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["work", "frobnicate", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "usage");
}
```

Run: `cargo test -p heiwa-shell --test cli_contract work`

Expected: FAIL. `work list --json` prints the bare `{"skipped_events":0,"work":[]}` with no schema, so `cli_v1::data` panics.

- [ ] **Step 2: Move `work` onto the contract**

In `apps/heiwa_shell/src/cmd/work.rs`:

(a) Below `use heiwa_work::{...};` add:

```rust
use crate::cmd::args::{flag_value, has_flag, positionals};
use crate::output::{self, CliError};
```

(b) Replace the `Some(other) => ...` arm of `run` with:

```rust
        Some(other) => Err(CliError::usage(format!("unknown work command: {other}"))
            .with_hint("run `heiwa work --help`")
            .into()),
```

(c) Replace `fn list` (lines 55-83) with:

```rust
fn list(args: &[String]) -> Result<()> {
    let paths = heiwa_config::HeiwaPaths::resolve();
    let summary = summarize(&paths.evidence_dir)?;
    let next = if summary["work"].as_array().map_or(true, Vec::is_empty) {
        vec!["heiwa work create \"<what you want done>\"".to_string()]
    } else {
        Vec::new()
    };
    output::emit(has_flag(args, "--json"), summary, &next, render_list);
    Ok(())
}

fn render_list(summary: &Value) {
    let works = summary["work"].as_array().cloned().unwrap_or_default();
    if works.is_empty() {
        println!("no Work on this installation yet");
        println!("  run `heiwa work create \"<what you want done>\"`");
    } else {
        for work in &works {
            println!(
                "{}  {}  rev {}",
                work["work_id"].as_str().unwrap_or("?"),
                work["status"].as_str().unwrap_or("?"),
                work["revision"].as_u64().unwrap_or(0),
            );
            println!("  {}", work["intent"].as_str().unwrap_or(""));
        }
    }
    let skipped = summary["skipped_events"].as_u64().unwrap_or(0);
    if skipped > 0 {
        println!();
        println!("! {skipped} work event(s) could not be folded; run `heiwa doctor` for detail");
    }
}
```

(d) Replace `fn create_command` (lines 85-107) with:

```rust
fn create_command(args: &[String]) -> Result<()> {
    let intent = positionals(args, &[])
        .first()
        .copied()
        .ok_or_else(|| CliError::usage("usage: heiwa work create \"<intent>\" [--json]"))?;
    let paths = heiwa_config::HeiwaPaths::resolve();
    let identity = heiwa_identity::load_from(&paths.runtime_root)
        .map_err(|error| anyhow!("{error}"))?
        .ok_or_else(|| {
            CliError::failure("no local identity on this installation")
                .with_hint("run `heiwa setup` before creating Work")
        })?;

    let created = create(&paths.evidence_dir, intent, &identity.installation_id)?;
    let work_id = created["work_id"].as_str().unwrap_or("?").to_string();
    let next = vec![format!("heiwa work show {work_id}")];
    output::emit(has_flag(args, "--json"), created, &next, |created| {
        println!("opened {}", created["work_id"].as_str().unwrap_or("?"));
        println!("  {}", created["intent"].as_str().unwrap_or(""));
    });
    Ok(())
}
```

(e) In `fn show_command`, replace everything from its first line down to and including the `if has_flag(args, "--json") { ... return Ok(()); }` block (lines 109-144) with the code below. Leave the human rendering that follows unchanged.

```rust
fn show_command(args: &[String]) -> Result<()> {
    let json = has_flag(args, "--json");
    let surface = flag_value(args, "--surface");
    let work_id = positionals(args, &["--surface"])
        .first()
        .copied()
        .ok_or_else(|| {
            CliError::usage("usage: heiwa work show <work-id> [--json | --surface <name>]")
        })?;
    let paths = heiwa_config::HeiwaPaths::resolve();
    if find(&paths.evidence_dir, work_id)?.is_none() {
        return Err(CliError::not_found(format!("no Work {work_id} on this installation"))
            .with_hint("list Work with `heiwa work list`")
            .into());
    }
    let epoch_seed = format!("cli-{}", uuid::Uuid::new_v4());
    if let Some(surface) = surface {
        let rendered = if surface == "all" {
            surfaces_json(&paths.evidence_dir, work_id, &epoch_seed)?
        } else {
            let snapshot = session(&paths.evidence_dir, work_id, &epoch_seed)?;
            let view = heiwa_work::view_for(&snapshot, surface).ok_or_else(|| {
                CliError::usage(format!(
                    "unknown surface {surface}; expected home, work, agent, or all"
                ))
            })?;
            serde_json::to_value(view)?
        };
        output::emit(json, rendered, &[], |rendered| {
            println!("{}", serde_json::to_string_pretty(rendered).unwrap_or_default());
        });
        return Ok(());
    }
    let snapshot = session(&paths.evidence_dir, work_id, &epoch_seed)?;
    if json {
        output::emit(true, serde_json::to_value(&snapshot)?, &[], |_| {});
        return Ok(());
    }
```

(f) Replace `fn recover_command` (lines 228-267) with:

```rust
fn recover_command(args: &[String]) -> Result<()> {
    let paths = heiwa_config::HeiwaPaths::resolve();
    let outcome = crate::cmd::recover::recover(&service(&paths.evidence_dir)?)?;
    let report = crate::cmd::recover::report(&outcome);
    output::emit(has_flag(args, "--json"), report, &[], render_recovery);
    Ok(())
}

fn render_recovery(report: &Value) {
    println!(
        "recovered {} interrupted turn(s); {} run(s) marked stale",
        report["interrupted_turns"].as_u64().unwrap_or(0),
        report["runs_marked_stale"].as_u64().unwrap_or(0)
    );
    for run in report["runs"].as_array().into_iter().flatten() {
        let described = describe_supervision_loss(&json!({ "supervision": run }));
        println!(
            "  run {}  {}  {described}",
            run["run_id"].as_str().unwrap_or("?"),
            run["work_id"].as_str().unwrap_or("?")
        );
    }
    for run in report["runs_withheld"].as_array().into_iter().flatten() {
        println!(
            "  run {}  {}  not marked: {}",
            run["run_id"].as_str().unwrap_or("?"),
            run["work_id"].as_str().unwrap_or("?"),
            run["reason"].as_str().unwrap_or("unknown evidence")
        );
    }
    let unadmitted = report["unadmitted_worker_events"]
        .as_array()
        .map_or(0, Vec::len);
    let unreadable = report["unreadable_journal_lines"].as_u64().unwrap_or(0);
    if unadmitted > 0 || unreadable > 0 {
        println!(
            "! {unadmitted} worker row(s) this build does not admit and {unreadable} unreadable journal line(s) were preserved uninterpreted"
        );
    }
}
```

(g) Delete the module-local `fn flag_value<'a>(...)` and `fn has_flag(...)` at the bottom of the file (lines 405-414). The shared helpers replace them.

(h) In `apps/heiwa_shell/src/registry.rs`, make the `work` entry declare its v1 invocations:

```rust
    CommandSpec {
        v1: &["work list", "work create", "work show", "work recover"],
        ..command("work", "work list|create|show|run|recover", "Durable Work on this installation")
    },
```

- [ ] **Step 3: Update the in-repo readers of `work` JSON**

In `apps/heiwa_shell/tests/work_support/mod.rs`:

(a) After `#![allow(dead_code)]`, add:

```rust
#[path = "../cli_v1/mod.rs"]
pub mod cli_v1;
```

(b) Replace:

```rust
    let created: serde_json::Value = serde_json::from_slice(&created.stdout).expect("create JSON");
```

with:

```rust
    let created = cli_v1::data(&created.stdout);
```

In `apps/heiwa_shell/tests/work_fabric_a1.rs`:

(a) Below the existing `fn json(...)` helper, add:

```rust
/// A successful `heiwa.cli/v1` command's `data`.
fn v1(output: std::process::Output, command: &str) -> Value {
    let output = successful(output, command);
    work_support::cli_v1::data(&output.stdout)
}
```

(b) In the surfaces call near line 49, change `json(` to `v1(` and the arguments to:

```rust
            &["work", "show", &work.work_id, "--surface", "all", "--json"],
```

(c) In the `work show --json` call near line 163, change `json(` to `v1(`.

(d) Replace every `json(recover(&work)` with `v1(recover(&work)`. There are six occurrences, near lines 227, 257, 261, 308, 394, and 462. Leave the `work run` call near line 190 on `json(`, since that command is not migrated.

Run: `grep -n 'json(recover' apps/heiwa_shell/tests/work_fabric_a1.rs`

Expected: no output.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p heiwa-shell --bin heiwa cmd::work && cargo test -p heiwa-shell --test cli_contract --test work_fabric_a1 --test work_run`

Expected: PASS. The existing `cmd::work` unit tests are unchanged and still pass.

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_shell/src/cmd/work.rs apps/heiwa_shell/src/registry.rs apps/heiwa_shell/tests/work_support/mod.rs apps/heiwa_shell/tests/work_fabric_a1.rs apps/heiwa_shell/tests/cli_contract.rs
git commit -m "feat(cli): heiwa work list/create/show/recover on heiwa.cli/v1"
```

---

### Task 5: `heiwa work watch`, a resumable event stream

**Files:**
- Modify: `apps/heiwa_shell/src/cmd/work.rs` (imports, `run`, `print_help`, `create_command` next hint, new functions, unit tests)
- Modify: `apps/heiwa_shell/src/registry.rs` (`work` entry)
- Test: `apps/heiwa_shell/tests/cli_contract.rs`

- [ ] **Step 1: Write the failing unit tests**

Append inside the existing `mod tests` in `apps/heiwa_shell/src/cmd/work.rs`:

```rust
    #[test]
    fn watching_reads_only_this_works_events_and_resumes_from_its_cursor() {
        let dir = root();
        let first = create(dir.path(), "first", "installation-1").expect("create first");
        let second = create(dir.path(), "second", "installation-1").expect("create second");
        let work = find(dir.path(), first["work_id"].as_str().expect("id"))
            .expect("find")
            .expect("first work exists");

        let page = watch_page(dir.path(), &work, None, 256).expect("page");
        assert!(
            page.lines.iter().any(|line| {
                line["event"]["event_type"] == "work_created" && line["scope"] == "work"
            }),
            "{:?}",
            page.lines
        );
        assert!(
            page.lines.iter().all(|line| {
                line["event"]["work_id"] != second["work_id"]
                    && line["event"]["thread_id"] != second["primary_thread_id"]
            }),
            "the other Work leaked into this stream: {:?}",
            page.lines
        );
        assert!(page
            .lines
            .iter()
            .all(|line| line["schema"] == "heiwa.cli.stream/v1" && line["type"] == "event"));

        let resumed = watch_page(dir.path(), &work, page.cursor.as_deref(), 256).expect("resume");
        assert!(resumed.lines.is_empty(), "{:?}", resumed.lines);
        assert_eq!(resumed.cursor, page.cursor, "an idle page keeps its place");
    }

    #[test]
    fn watching_reports_activity_appended_after_the_cursor() {
        let dir = root();
        let created = create(dir.path(), "watched", "installation-1").expect("create");
        let work_id = created["work_id"].as_str().expect("id").to_string();
        let thread_id = created["primary_thread_id"].as_str().expect("thread").to_string();
        let work = find(dir.path(), &work_id).expect("find").expect("work");
        let caught_up = watch_page(dir.path(), &work, None, 256).expect("page").cursor;

        let service = service(dir.path()).expect("service");
        let mut request = heiwa_session::operator::StartTurnRequest::auto("work-watch", "keep going");
        request.work_id = Some(work_id.clone());
        service.start_turn(&thread_id, request).expect("start turn");

        let page = watch_page(dir.path(), &work, caught_up.as_deref(), 256).expect("next page");
        assert!(
            page.lines.iter().any(|line| line["event"]["event_type"] == "turn_started"),
            "{:?}",
            page.lines
        );
        assert!(page.lines.iter().all(|line| line["event"]["thread_id"] == thread_id.as_str()));
        assert_ne!(page.cursor, caught_up);
    }

    #[test]
    fn a_foreign_cursor_is_a_usage_error() {
        let dir = root();
        let created = create(dir.path(), "watched", "installation-1").expect("create");
        let work = find(dir.path(), created["work_id"].as_str().expect("id"))
            .expect("find")
            .expect("work");
        let error = match watch_page(dir.path(), &work, Some("not-a-cursor"), 256) {
            Ok(_) => panic!("a foreign cursor must be refused"),
            Err(error) => error,
        };
        assert_eq!(
            crate::output::classify(&error).code,
            crate::output::ErrorCode::Usage,
            "{error:#}"
        );
    }
```

Run: `cargo test -p heiwa-shell --bin heiwa cmd::work`

Expected: FAIL to compile with `cannot find function 'watch_page'`.

- [ ] **Step 2: Implement the stream**

In `apps/heiwa_shell/src/cmd/work.rs`:

(a) Change the `heiwa_evidence` import and the `output` import to:

```rust
use heiwa_evidence::{CursorError, CursorEvent, OperatorEvent, OperatorJournal};
```

```rust
use crate::output::{self, CliError, STREAM_SCHEMA};
```

(b) In `run`, add after the `Some("show")` arm:

```rust
        Some("watch") => watch_command(&args[1..]),
```

(c) In `print_help`, add after the `work show <work-id> --surface` lines:

```rust
    println!("  heiwa work watch <work-id> [--since <cursor>] [--once] [--json]");
    println!("                                        stream this Work's events; resumable");
```

(d) In `create_command`, change the `next` line to:

```rust
    let next = vec![
        format!("heiwa work show {work_id}"),
        format!("heiwa work watch {work_id}"),
    ];
```

(e) Add these functions after `recover_command`/`render_recovery`:

```rust
/// One page of a Work's events after a cursor, and where to resume.
pub(crate) struct WatchPage {
    pub(crate) lines: Vec<Value>,
    pub(crate) cursor: Option<String>,
}

/// Why an event belongs to `work`. `"work"` means it carries the Work's id.
/// `"thread"` means it happened in one of the Work's threads without naming
/// the Work: an unscoped turn, shown rather than hidden.
fn watch_scope(event: &OperatorEvent, work: &Work) -> Option<&'static str> {
    if event.work_id.as_deref() == Some(work.work_id.as_str()) {
        Some("work")
    } else if event.thread_id == work.primary_thread_id
        || work.related_thread_ids.contains(&event.thread_id)
    {
        Some("thread")
    } else {
        None
    }
}

/// Read one journal page after `since` and keep this Work's events.
///
/// The cursor advances past every row read, including rows for other Work,
/// and stays put when nothing was read. So a follower never re-reads and never
/// loses its place.
pub(crate) fn watch_page(
    root: &Path,
    work: &Work,
    since: Option<&str>,
    limit: usize,
) -> Result<WatchPage> {
    let journal = OperatorJournal::new(root.to_path_buf()).map_err(|error| anyhow!("{error}"))?;
    let page = journal.read_after(since, limit).map_err(|error| match error {
        CursorError::InvalidCursor { reason } => anyhow::Error::new(
            CliError::usage(format!("invalid --since cursor: {reason}"))
                .with_hint("resume from a cursor printed by `heiwa work watch`"),
        ),
        other => anyhow!("{other}"),
    })?;
    let mut cursor = since.map(str::to_string);
    let mut lines = Vec::new();
    for CursorEvent {
        cursor: row_cursor,
        event,
    } in page.events
    {
        if let Some(scope) = watch_scope(&event, work) {
            lines.push(json!({
                "schema": STREAM_SCHEMA,
                "type": "event",
                "scope": scope,
                "cursor": row_cursor,
                "event": event,
            }));
        }
        cursor = Some(row_cursor);
    }
    Ok(WatchPage { lines, cursor })
}

fn watch_command(args: &[String]) -> Result<()> {
    const PAGE_SIZE: usize = 256;
    const POLL: std::time::Duration = std::time::Duration::from_millis(500);

    let work_id = positionals(args, &["--since"])
        .first()
        .copied()
        .ok_or_else(|| {
            CliError::usage("usage: heiwa work watch <work-id> [--since <cursor>] [--once] [--json]")
        })?;
    let json = has_flag(args, "--json");
    let once = has_flag(args, "--once");
    let paths = heiwa_config::HeiwaPaths::resolve();
    let work = find(&paths.evidence_dir, work_id)?.ok_or_else(|| {
        CliError::not_found(format!("no Work {work_id} on this installation"))
            .with_hint("list Work with `heiwa work list`")
    })?;

    let mut cursor = flag_value(args, "--since").map(str::to_string);
    loop {
        let page = watch_page(&paths.evidence_dir, &work, cursor.as_deref(), PAGE_SIZE)?;
        let advanced = page.cursor != cursor;
        for line in &page.lines {
            print_watch_line(line, json);
        }
        cursor = page.cursor;
        if advanced {
            continue;
        }
        if once {
            print_watch_end(work_id, cursor.as_deref(), json);
            return Ok(());
        }
        std::io::Write::flush(&mut std::io::stdout())?;
        std::thread::sleep(POLL);
    }
}

fn print_watch_line(line: &Value, json: bool) {
    if json {
        println!("{line}");
        return;
    }
    let event = &line["event"];
    println!(
        "{}  {:<22} {:<6} {}",
        event["occurred_at"].as_str().unwrap_or("?"),
        event["event_type"].as_str().unwrap_or("?"),
        line["scope"].as_str().unwrap_or("?"),
        event["actor"]["id"].as_str().unwrap_or("?"),
    );
}

fn print_watch_end(work_id: &str, cursor: Option<&str>, json: bool) {
    if json {
        println!(
            "{}",
            json!({
                "schema": STREAM_SCHEMA,
                "type": "end",
                "reason": "caught_up",
                "cursor": cursor,
            })
        );
    } else if let Some(cursor) = cursor {
        println!("caught up; resume with: heiwa work watch {work_id} --since {cursor}");
    } else {
        println!("caught up; no events yet");
    }
}
```

(f) In `apps/heiwa_shell/src/registry.rs`, update the `work` entry:

```rust
    CommandSpec {
        v1: &["work list", "work create", "work show", "work watch", "work recover"],
        ..command("work", "work list|create|show|watch|run|recover", "Durable Work on this installation")
    },
```

- [ ] **Step 3: Run the unit tests to verify they pass**

Run: `cargo test -p heiwa-shell --bin heiwa cmd::work`

Expected: PASS, including the 3 new tests.

- [ ] **Step 4: Write the failing binary test**

Append to `apps/heiwa_shell/tests/cli_contract.rs`:

```rust
fn home_with_identity() -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("home");
    heiwa_identity::establish_in(
        &home.path().join(".heiwa"),
        "Test operator",
        "2026-09-27T00:00:00Z",
        || "install-test".to_string(),
    )
    .expect("identity");
    home
}

fn ndjson(stdout: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each stream line is JSON"))
        .collect()
}

#[test]
fn watching_once_streams_events_then_an_end_line_to_resume_from() {
    let home = home_with_identity();
    let created = heiwa(home.path(), &["work", "create", "watch me", "--json"]);
    assert!(created.status.success(), "{}", stderr(&created));
    let work_id = cli_v1::data(&created.stdout)["work_id"]
        .as_str()
        .expect("work id")
        .to_string();

    let watched = heiwa(home.path(), &["work", "watch", &work_id, "--once", "--json"]);
    assert!(watched.status.success(), "{}", stderr(&watched));
    let lines = ndjson(&watched.stdout);
    assert!(
        lines.iter().all(|line| line["schema"] == "heiwa.cli.stream/v1"),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line["type"] == "event" && line["event"]["event_type"] == "work_created"),
        "{lines:?}"
    );
    let end = lines.last().expect("an end line");
    assert_eq!(end["type"], "end");
    let cursor = end["cursor"].as_str().expect("resume cursor").to_string();

    let resumed = heiwa(
        home.path(),
        &["work", "watch", &work_id, "--since", &cursor, "--once", "--json"],
    );
    assert!(resumed.status.success(), "{}", stderr(&resumed));
    let resumed_lines = ndjson(&resumed.stdout);
    assert_eq!(resumed_lines.len(), 1, "only the end line: {resumed_lines:?}");
    assert_eq!(resumed_lines[0]["type"], "end");
    assert_eq!(resumed_lines[0]["cursor"], cursor.as_str());
}
```

- [ ] **Step 5: Run it to verify it passes**

Run: `cargo test -p heiwa-shell --test cli_contract watching`

Expected: PASS. Steps 1-3 already implemented the stream; this step proves it end to end through the binary.

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_shell/src/cmd/work.rs apps/heiwa_shell/src/registry.rs apps/heiwa_shell/tests/cli_contract.rs
git commit -m "feat(cli): heiwa work watch — resumable NDJSON Work event stream"
```

---

### Task 6: `heiwa calendar plan` on the contract

**Files:**
- Modify: `apps/heiwa_shell/src/cmd/calendar_plan.rs:425-476` (`run`), plus new `staged_next` and `render_staged`, and the tests module near line 673
- Modify: `apps/heiwa_shell/src/registry.rs` (`calendar` entry)

- [ ] **Step 1: Write the failing unit tests**

Append inside `mod tests` in `apps/heiwa_shell/src/cmd/calendar_plan.rs`:

```rust
    #[test]
    fn a_plan_command_without_arguments_is_a_usage_error() {
        let error = run(&[]).expect_err("usage");
        assert_eq!(
            crate::output::classify(&error).code,
            crate::output::ErrorCode::Usage
        );
    }

    #[test]
    fn a_staged_plan_points_at_its_approval() {
        let staged = json!({"in_sync": false, "approval_request": {"request_id": "req_1"}});
        assert_eq!(
            staged_next(&staged),
            vec![
                "heiwa approvals show req_1".to_string(),
                "heiwa approvals decide req_1 --approve".to_string()
            ]
        );
        assert!(staged_next(&json!({"in_sync": true})).is_empty());
    }
```

Run: `cargo test -p heiwa-shell --bin heiwa cmd::calendar_plan`

Expected: FAIL to compile with `cannot find function 'staged_next'`.

- [ ] **Step 2: Implement**

In `apps/heiwa_shell/src/cmd/calendar_plan.rs`:

(a) Add to the imports:

```rust
use crate::cmd::args::{has_flag, positionals};
use crate::output::{self, CliError};
```

(b) Replace `pub(crate) fn run` (lines 425-476) with:

```rust
pub(crate) fn run(args: &[String]) -> Result<()> {
    let usage = "usage: heiwa calendar plan diff|stage <plan.json> [--adopt] [--json]";
    let positional = positionals(args, &[]);
    let (Some(sub), Some(path)) = (positional.first().copied(), positional.get(1).copied()) else {
        return Err(CliError::usage(usage).into());
    };
    let adopt = has_flag(args, "--adopt");
    let as_json = output::wants_json(args);
    match sub {
        "diff" => {
            super::connectors::require_apple_calendar_connection()?;
            let plan = load_plan(path)?;
            let (marked, unmarked) = scan(&plan, adopt)?;
            let changes = diff(&plan, &marked, &unmarked, adopt)?;
            let counts = counts(&changes, &plan);
            let data = json!({
                "plan_id": plan.plan_id,
                "counts": counts,
                "changes": review_rows(&changes),
            });
            let next = vec![format!("heiwa calendar plan stage {path}")];
            output::emit(as_json, data, &next, |_| {
                println!("calendar plan diff: {}", plan.plan_id);
                println!("  {}", summary_line(&counts));
                print_rows(&changes);
            });
            Ok(())
        }
        "stage" => {
            let staged = stage(path, adopt)?;
            let next = staged_next(&staged);
            output::emit(as_json, staged, &next, render_staged);
            Ok(())
        }
        other => Err(CliError::usage(format!("unknown calendar plan command: {other}"))
            .with_hint(usage)
            .into()),
    }
}

/// Follow-up commands for a staged plan: review, then decide.
fn staged_next(staged: &Value) -> Vec<String> {
    match staged["approval_request"]["request_id"].as_str() {
        Some(id) if staged["in_sync"] != true => vec![
            format!("heiwa approvals show {id}"),
            format!("heiwa approvals decide {id} --approve"),
        ],
        _ => Vec::new(),
    }
}

fn render_staged(staged: &Value) {
    if staged["in_sync"] == true {
        println!(
            "calendar plan {}: already in sync, nothing to approve",
            staged["plan_id"].as_str().unwrap_or("?")
        );
        return;
    }
    let id = staged["approval_request"]["request_id"]
        .as_str()
        .unwrap_or("?");
    println!("calendar plan staged: {id} (T2)");
    println!(
        "  {}",
        summary_line(&staged["approval_request"]["intent"]["counts"])
    );
    println!("  review: heiwa approvals show {id}");
    println!("  apply:  heiwa approvals decide {id} --approve");
}
```

If the compiler reports `Value` not in scope, add `Value` to the existing `serde_json` import (`use serde_json::{json, Value};`).

(c) In `apps/heiwa_shell/src/registry.rs`, update the `calendar` entry:

```rust
    CommandSpec {
        v1: &["calendar plan diff", "calendar plan stage"],
        ..command("calendar", "calendar status|sync|hold|plan", "Calendar lanes, local holds, and plan sync")
    },
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test -p heiwa-shell --bin heiwa cmd::calendar_plan && cargo test -p heiwa-shell --test calendar_plan_sync && cargo test -p heiwa-shell --bin heiwa registry::`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/heiwa_shell/src/cmd/calendar_plan.rs apps/heiwa_shell/src/registry.rs
git commit -m "feat(cli): heiwa calendar plan diff/stage on heiwa.cli/v1"
```

---

### Task 7: `heiwa approvals` rendering on the contract

This task changes rendering only. Do not modify `decide_request`, `compute_effects`, `summarize_effects`, the `"local-cli"` argument, or any file handling. That is C1-a's surface.

**Files:**
- Modify: `apps/heiwa_shell/src/cmd/approvals.rs:1-159` (imports, `run`, `list`, `show`, `decide` rendering)
- Modify: `apps/heiwa_shell/src/registry.rs` (`approvals` entry)
- Modify: `apps/heiwa_shell/tests/smoke.rs`, `tests/schedule.rs`, `tests/mail_triage.rs`, `tests/apple_calendar_connector.rs`
- Test: `apps/heiwa_shell/tests/cli_contract.rs`

- [ ] **Step 1: Write the failing binary tests**

Append to `apps/heiwa_shell/tests/cli_contract.rs`:

```rust
#[test]
fn approvals_list_is_an_envelope() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["approvals", "list", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(cli_v1::data(&output.stdout)["pending_summary"], serde_json::json!([]));
}

#[test]
fn showing_an_unknown_approval_is_not_found() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["approvals", "show", "req_missing", "--json"]);
    assert_eq!(output.status.code(), Some(10), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "not_found");
}

#[test]
fn an_approval_id_cannot_walk_out_of_the_requests_directory() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["approvals", "show", "../../secrets", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "usage");
}
```

Run: `cargo test -p heiwa-shell --test cli_contract approval`

Expected: FAIL, because `approvals list --json` prints `{"command":"approvals list",...}` with no schema.

- [ ] **Step 2: Implement the rendering changes**

In `apps/heiwa_shell/src/cmd/approvals.rs`:

(a) Add to the imports:

```rust
use crate::output::{self, CliError};
```

(b) Replace the `Some(other) => ...` arm of `run` with:

```rust
        Some(other) => Err(CliError::usage(format!("unknown approvals command: {other}"))
            .with_hint("run `heiwa approvals --help`")
            .into()),
```

(c) Replace `fn list` (lines 21-56) with:

```rust
fn list(args: &[String]) -> Result<()> {
    let pending = scan_pending_requests();
    let decisions = scan_decisions();
    let pending_summary: Vec<Value> = pending.iter().map(approval_request_summary).collect();
    let next: Vec<String> = pending_summary
        .iter()
        .filter_map(|summary| summary["id"].as_str())
        .take(10)
        .map(|id| format!("heiwa approvals show {id}"))
        .collect();
    let data = json!({
        "requests_dir": requests_dir().display().to_string(),
        "decisions_dir": decisions_dir().display().to_string(),
        "pending": pending,
        "pending_summary": pending_summary,
        "decided": decisions,
    });
    output::emit(has_flag(args, "--json"), data, &next, render_list);
    Ok(())
}

fn render_list(data: &Value) {
    let pending = data["pending_summary"].as_array().cloned().unwrap_or_default();
    println!("approvals");
    println!("  requests: {} pending", pending.len());
    println!(
        "  decisions: {} on record",
        data["decided"].as_array().map_or(0, Vec::len)
    );
    println!("  requests dir: {}", data["requests_dir"].as_str().unwrap_or("?"));
    println!("  decisions dir: {}", data["decisions_dir"].as_str().unwrap_or("?"));
    for summary in pending.iter().take(10) {
        let id = summary.get("id").and_then(Value::as_str).unwrap_or("?");
        let action = summary.get("action").and_then(Value::as_str).unwrap_or("?");
        let target = summary.get("target").and_then(Value::as_str).unwrap_or("?");
        let risk = summary.get("risk").and_then(Value::as_str).unwrap_or("?");
        println!("    {id}  {action} -> {target}  risk={risk}");
    }
    if pending.len() > 10 {
        println!("    ... {} more", pending.len() - 10);
    }
}
```

(d) Replace `fn show` (lines 58-75) with the version below. It validates the id before touching the filesystem. Previously `show` joined an unvalidated id into a path.

```rust
fn show(args: &[String]) -> Result<()> {
    let id = args
        .first()
        .filter(|arg| !arg.starts_with("--"))
        .ok_or_else(|| CliError::usage("usage: heiwa approvals show <id> [--json]"))?;
    validate_request_id(id).map_err(|error| CliError::usage(format!("{error}")))?;
    let path = requests_dir().join(format!("{id}.json"));
    if !path.exists() {
        return Err(CliError::not_found(format!("approval not found: {id}"))
            .with_hint("list pending approvals with `heiwa approvals list`")
            .into());
    }
    let raw = fs::read_to_string(&path)?;
    let value: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
    let next = vec![format!("heiwa approvals decide {id} --approve|--deny")];
    output::emit(has_flag(args, "--json"), value, &next, |value| {
        println!("approval {id}");
        println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
    });
    Ok(())
}
```

(e) In `fn decide`, change only the usage error and the two printing blocks.

Replace the `let id = args.first().ok_or_else(|| { anyhow!(...) })?;` statement with:

```rust
    let id = args.first().filter(|arg| !arg.starts_with("--")).ok_or_else(|| {
        CliError::usage("usage: heiwa approvals decide <id> --approve|--deny [--note ...]")
    })?;
```

Replace `return Err(anyhow!("must pass exactly one of --approve or --deny"));` with:

```rust
        return Err(CliError::usage("must pass exactly one of --approve or --deny").into());
```

Replace the dry-run printing block (`if has_flag(args, "--json") { ... } else { ... }` inside `if dry_run`) with:

```rust
        let data = json!({
            "dry_run": true,
            "path": path.display().to_string(),
            "decision": decision,
        });
        output::emit(has_flag(args, "--json"), data, &[], |_| {
            println!("approvals decide (dry-run)");
            println!("  id: {id}");
            println!("  outcome: {}", decision["outcome"]);
            println!("  effects: {}", summarize_effects(&plan));
            for effect in plan.as_array().into_iter().flatten() {
                println!(
                    "    - {}: {} -> {}",
                    effect.get("surface").and_then(Value::as_str).unwrap_or("?"),
                    effect.get("target").and_then(Value::as_str).unwrap_or("?"),
                    effect.get("change").and_then(Value::as_str).unwrap_or("?")
                );
            }
            println!("  would write: {}", path.display());
        });
```

Replace the final printing block (everything after `let applied = decision_out["applied_effects"].clone();` down to the closing `Ok(())`) with:

```rust
    let data = json!({
        "dry_run": false,
        "path": path.display().to_string(),
        "decision": decision_out,
    });
    output::emit(has_flag(args, "--json"), data, &[], |_| {
        println!("approvals decide");
        println!("  id: {id}");
        println!("  outcome: {}", decision_out["outcome"]);
        if let Some(applied_arr) = applied.as_array() {
            if !applied_arr.is_empty() {
                println!("  applied:");
                for effect in applied_arr {
                    println!(
                        "    - {}",
                        effect.get("summary").and_then(Value::as_str).unwrap_or("?")
                    );
                }
            }
        }
        println!("  wrote: {}", path.display());
    });
    Ok(())
```

Leave the line `let result = decide_request(id, approve, flag_value(args, "--note"), "local-cli")?;` exactly as it is.

(f) In `apps/heiwa_shell/src/registry.rs`, update the `approvals` entry:

```rust
    CommandSpec {
        v1: &["approvals list", "approvals show", "approvals decide"],
        ..command("approvals", "approvals list|show|decide", "Review and decide staged actions")
    },
```

- [ ] **Step 3: Update the in-repo readers of `approvals` JSON**

Add `mod cli_v1;` after the `use` lines at the top of each of `apps/heiwa_shell/tests/smoke.rs`, `tests/schedule.rs`, `tests/mail_triage.rs`, and `tests/apple_calendar_connector.rs`.

In `tests/smoke.rs`, `test_approvals_list_json_reports_dispatch_paths`: replace everything after `assert!(output.status.success());` down to the end of the function with:

```rust
    let data = cli_v1::data(&output.stdout);
    let requests_dir = data["requests_dir"].as_str().expect("requests_dir").replace('\\', "/");
    let decisions_dir = data["decisions_dir"].as_str().expect("decisions_dir").replace('\\', "/");
    assert!(requests_dir.contains("dispatch/requests"), "{data}");
    assert!(
        decisions_dir.contains("dispatch/approvals/decisions"),
        "{data}"
    );
}
```

In `tests/smoke.rs`, `test_approvals_list_json_reports_dispatch_v1_summary`: replace

```rust
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("approvals list --json must be valid JSON");
```

with:

```rust
    let parsed = cli_v1::data(&output.stdout);
```

In `tests/schedule.rs`, replace:

```rust
    let listing: serde_json::Value = serde_json::from_slice(&out.stdout).expect("approvals json");
```

with:

```rust
    let listing = cli_v1::data(&out.stdout);
```

In `tests/mail_triage.rs`, replace:

```rust
    let decision: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
```

with:

```rust
    let decision = cli_v1::data(stdout.trim().as_bytes());
```

In `tests/apple_calendar_connector.rs`, replace:

```rust
    let approved: serde_json::Value =
        serde_json::from_slice(&approved.stdout).expect("approval JSON");
```

with:

```rust
    let approved = cli_v1::data(&approved.stdout);
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p heiwa-shell --test cli_contract --test smoke --test schedule --test mail_triage --test apple_calendar_connector --test approvals_decide --test approval_gate && cargo test -p heiwa-shell --bin heiwa cmd::approvals`

Expected: PASS. `approvals_decide.rs` needs no edit: it asserts the conflict message on stderr, and `report_error` still prints it there.

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_shell/src/cmd/approvals.rs apps/heiwa_shell/src/registry.rs apps/heiwa_shell/tests/cli_contract.rs apps/heiwa_shell/tests/smoke.rs apps/heiwa_shell/tests/schedule.rs apps/heiwa_shell/tests/mail_triage.rs apps/heiwa_shell/tests/apple_calendar_connector.rs
git commit -m "feat(cli): heiwa approvals on heiwa.cli/v1; validate ids in show"
```

---

### Task 8: Documentation, ledger, and full verification

**Files:**
- Modify: `docs/design/refs/CLI.md` (after `## Exit Codes`)
- Modify: `docs/superpowers/ledgers/2026-08-22-work-fabric-task-ledger.md` (Release C1 table)

- [ ] **Step 1: Document the contract**

In `docs/design/refs/CLI.md`, add this line to the `## Exit Codes` list, before `10+ — verb-specific`:

```markdown
- `10` — not found: the named object does not exist
```

Then insert this section directly after the `## Exit Codes` list:

```markdown
## Result Contract (`heiwa.cli/v1`)

Invocations listed under `v1` in `heiwa help --json` follow this contract.
Other commands keep their current output until they migrate.

- `--json` prints exactly one envelope on stdout:
  - success: `{"schema": "heiwa.cli/v1", "ok": true, "data": {...}, "next": ["<suggested command>"]}`
  - error: `{"schema": "heiwa.cli/v1", "ok": false, "error": {"code": "usage", "message": "...", "hint": "..."}, "next": []}`
- On error, a readable diagnostic always goes to stderr, with or without `--json`.
- `error.code` names the exit code: `failure` 1, `usage` 2, `not_found` 10.
- Streams such as `heiwa work watch --json` print NDJSON lines with
  `"schema": "heiwa.cli.stream/v1"`. Every `event` line carries the `cursor` to
  resume from. `--once` ends with `{"type": "end", "cursor": ...}`, and
  `--since <cursor>` continues from it.
- `next` suggests follow-up commands. It is advice, never authority: a
  suggested `approvals decide` still goes through the approval service and
  its policy.
- Migrated commands never prompt and never read stdin, so they are safe in
  pipelines and agent tool calls. A TTY is not authentication.
```

- [ ] **Step 2: Run the full verification**

Run each of these and record its result:

```bash
cargo fmt -p heiwa-shell -- --check
cargo clippy -p heiwa-shell --all-targets 2>&1 | grep -E 'src/(output|registry)\.rs|src/cmd/(args|work|calendar_plan|approvals)\.rs|tests/cli_' || echo "no clippy findings in touched files"
cargo test -p heiwa-shell --bin heiwa
cargo test -p heiwa-shell --test cli_contract --test smoke --test schedule --test mail_triage --test apple_calendar_connector --test approvals_decide --test approval_gate --test work_fabric_a1 --test work_run --test calendar_plan_sync
bash scripts/ci_rust_test_group.sh --check
```

Expected:
- `fmt` is clean. If it isn't, run `cargo fmt -p heiwa-shell` and re-run.
- There are no clippy findings in touched files.
- All tests pass and the group check exits 0.

- [ ] **Step 3: Record the ledger row**

In `docs/superpowers/ledgers/2026-08-22-work-fabric-task-ledger.md`, add this row to the Release C1 table, after the `C1-d` row:

```markdown
| C1-d1 | CLI contract: `heiwa.cli/v1` envelope and exit codes, `help --json` catalog, `work watch` stream; `work`, `calendar plan`, `approvals` migrated | done | `cargo test -p heiwa-shell --bin heiwa` and `cargo test -p heiwa-shell --test cli_contract` pass locally; plan `docs/superpowers/plans/2026-09-27-engines-c1d1-cli-contract.md`. Other commands keep legacy output until migrated. |
```

- [ ] **Step 4: Run the baseline and commit**

Run: `HEIWA_BRANCH_MODE=experimental bash scripts/check_agent_baseline.sh --branch experimental/heiwa-engines`

Expected: every check `OK` except `cached remote ref missing` while the branch is unpublished.

```bash
git add docs/design/refs/CLI.md docs/superpowers/ledgers/2026-08-22-work-fabric-task-ledger.md
git commit -m "docs(cli): heiwa.cli/v1 result contract; ledger C1-d1"
```

---

## Execution Notes (2026-09-28)

Before implementation I reviewed the plan against the real
`OperatorJournal::read_after`. Codex asked for this review. Where the notes
below and the task code differ, the implemented commits supersede the embedded
code.

1. **Output never panics on a closed pipe.**
   - `output::print_line` writes and flushes stdout. It returns `Ok(false)` when
     the reader has gone away (`BrokenPipe`), so a `… --json | head -1`
     pipeline ends quietly instead of panicking inside `println!`.
   - `output::emit` therefore returns `anyhow::Result<()>`, and call sites
     return it.
   - Human renderers still use `println!`, which is unchanged pre-existing
     behavior.
2. **Cursor semantics come from the journal, not the plan.**
   - A cursor is `{version, fingerprint of the stream's first line, byte
     offset}`.
   - `read_after` rejects a cursor whose fingerprint no longer matches (the
     stream was repaired, replaced, or compacted), whose offset is past the end
     or off an event boundary, or whose lineage changes mid-read. All of these
     arrive as `CursorError::InvalidCursor`, which is the same variant used for
     undecodable input.
   - `work watch` therefore separates two cases:
     - **Malformed** (the reason starts `cursor is not valid base64` or
       `cursor payload is malformed`): usage error, exit 2.
     - **Expired** (any other `InvalidCursor`, including a version from an
       older binary): emit `{"type": "resync", "reason": ..., "cursor": null}`
       and replay the Work from the start. Consumers de-duplicate by
       `event.event_id`.
   - Consecutive resyncs are capped at 3, after which the command fails.
     `UnstableLineage` is a failure.
   - Tests pin both classes against the real journal, so rewording in
     `heiwa_evidence` fails loudly instead of misclassifying.
3. **The resume point is `page.next_cursor`.** The journal documents it as
   never regressing: when a page has no events it echoes the input cursor.
4. **Threads linked after start stay visible.** The watcher tracks the Work's
   threads as it goes. Every event carrying the Work's id adds its thread, so
   unscoped events in threads linked after the watch began are included.
5. **A torn trailing line is safe.** It is a write still in progress. The
   journal stops before it and re-reads it once it is complete. `skipped_lines`
   is therefore not surfaced per page, because it would flicker during
   concurrent writes.
