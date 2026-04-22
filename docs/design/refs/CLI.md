# Heiwa CLI — Unified Verb Spec

Updated: 2026-04-22
Status: working spec for the single `heiwa` binary and its REPL

If this conflicts with [`HEIWA.md`](/Users/dmcgregsauce/heiwa-universe/HEIWA.md) or [`BRAND.md`](./BRAND.md), those win.

## Purpose

Define one coherent verb surface for the installed `heiwa` runtime. Operators learn one binary; every other surface (cockpit app, subagents, scheduled routines) is a view into the same verbs.

## Binary Layout

- `heiwa` — the single operator-facing binary, installed locally
- no separate `heiwa-route`, `heiwa-agent`, `heiwa-cron` binaries in the long run; the current `~/.heiwa/bin/heiwa-route` is a legacy shim that should fold under `heiwa route`
- subprocess shells and provider CLIs are spawned by `heiwa`, not called by operators directly for routed work

## Verb Table

| Verb        | Shape                               | What it does                                                                 |
|-------------|-------------------------------------|------------------------------------------------------------------------------|
| `heiwa`     | no args → REPL                      | Open the interactive operator REPL.                                          |
| `run`       | `heiwa run <prompt\|@file>`         | One-shot execution. Streams to stdout. Exits when done.                      |
| `repl`      | `heiwa repl`                        | Explicit alias for `heiwa` with no args. Useful inside scripts.              |
| `loop`      | `heiwa loop <spec>`                 | Fast feedback flow — run a spec in a loop with stop conditions.              |
| `cron`      | `heiwa cron {add\|ls\|rm\|run}`     | Schedule a verb to run on cron/rrule. Runs under the local scheduler.        |
| `agent`     | `heiwa agent {spawn\|ls\|kill\|attach}` | Manage subagent sessions. Parent sessions own their children.            |
| `route`     | `heiwa route {status\|show\|test}`  | Inspect routing decisions. No side effects in `status`/`show`.               |
| `providers` | `heiwa providers {ls\|link\|unlink\|test}` | Manage connected providers (OAuth CLIs, API keys, local runtimes).    |
| `memory`    | `heiwa memory {ls\|show\|rm\|ingest}` | Operate on durable Heiwa memory (user, project, session).                  |
| `trace`     | `heiwa trace {ls\|show\|export}`    | Inspect evidence and receipts for prior work.                                |
| `approvals` | `heiwa approvals {ls\|grant\|deny}` | Resolve pending approvals from running sessions.                             |
| `app`       | `heiwa app [--port N] [--open]`     | Start the local cockpit HTTP server; open browser if `--open`.               |
| `install`   | `heiwa install`                     | Idempotent runtime bootstrap. Safe to re-run.                                |
| `update`    | `heiwa update [--channel stable\|nightly]` | Self-update via GitHub Releases. Verifies signature.                  |
| `doctor`    | `heiwa doctor`                      | Diagnose the local install; print a redactable report.                        |
| `config`    | `heiwa config {get\|set\|path}`     | Read or modify `~/.heiwa/config.toml`.                                       |
| `plugin`    | `heiwa plugin {install\|ls\|rm}`    | Install plugins/skills. `gh:` scheme supported.                              |
| `browse`    | `heiwa browse <task>`               | (Planned) browser automation via local vision model, escalation on failure.  |
| `help`      | `heiwa help [verb]`                 | Built-in help. Short, example-led.                                           |
| `version`   | `heiwa version`                     | Semver + git hash + channel.                                                 |

## Command Conventions

- Verbs are one word, English, imperative. No camelCase, no dots.
- Subcommands use `heiwa <verb> <subverb>`, not `heiwa <verb>:<sub>`.
- Output is concise by default. `--verbose` for full detail, `--quiet` for status-code-only.
- Output format: `--json` switches to stable JSON for scripts.
- Interactive prompts only when stdin is a TTY. Never block in pipelines.
- Long-running verbs (`run`, `loop`, `agent spawn`) accept `--detach` to return a session id and release the terminal.
- Destructive verbs (`memory rm`, `agent kill`, `cron rm`) require `--yes` or interactive confirm. No silent deletes.

## Flag Conventions

Shared flags that behave identically across verbs:

| Flag              | Meaning                                                                    |
|-------------------|----------------------------------------------------------------------------|
| `--json`          | Emit stable JSON instead of human output.                                  |
| `--verbose` / `-v`| Verbose output. Stacks: `-vv`, `-vvv`.                                     |
| `--quiet` / `-q`  | Suppress non-error output.                                                 |
| `--yes` / `-y`    | Bypass interactive confirmation for destructive actions.                   |
| `--detach` / `-d` | Return a session id, release the terminal, keep running in the background.|
| `--provider <id>` | Force a specific provider lane.                                            |
| `--model <id>`    | Force a specific model (provider-scoped).                                  |
| `--route <role>`  | Pick a routing role: `code`, `chat`, `reason`, `review`.                   |
| `--project <path>`| Operate against a specific project root, not cwd.                          |

## Exit Codes

- `0` — success
- `1` — generic failure
- `2` — usage error
- `3` — provider auth failure
- `4` — approval required and not granted
- `5` — sandbox required and unavailable
- `10+` — verb-specific

## Environment

- `HEIWA_HOME` — overrides `~/.heiwa/` (state root)
- `HEIWA_CONFIG` — overrides `~/.heiwa/config.toml`
- `HEIWA_PROVIDER` — default provider override
- `HEIWA_ROUTE_<ROLE>` — override a route (e.g. `HEIWA_ROUTE_CODE=ollama/qwen3.5:9b`)
- `NO_COLOR`, `CLICOLOR_FORCE` — standard color control
- `HEIWA_OFFLINE=1` — hard-disable any network provider; only local lanes allowed

## REPL Shape

`heiwa` with no args enters a REPL. Inside the REPL:

- bare text → routed as a prompt under the default route
- `/verb ...` → invokes a CLI verb directly (e.g. `/providers ls`, `/route status`)
- `!shell-cmd` → executes a local shell command, output returns to the REPL
- `@agent-id ...` → sends to a specific subagent session
- `:exit` or Ctrl-D → leave the REPL

REPL state (history, current project, routing overrides) persists per session id under `~/.heiwa/sessions/`.

## Examples

```bash
# Smallest useful thing
heiwa run "summarize CHANGELOG.md"

# Force a lane
heiwa run --route code --provider ollama "scan for TODO comments"

# Loop until a condition
heiwa loop --until "tests pass" "fix failing specs one at a time"

# Schedule a daily digest
heiwa cron add --at "06:30" heiwa run @prompts/morning-digest.md

# Spawn a subagent, detached
heiwa agent spawn --detach --name reviewer heiwa run @prompts/review-branch.md

# Open the cockpit
heiwa app --open

# Inspect current routing decisions
heiwa route status

# Link a provider via OAuth
heiwa providers link claude-code

# Self-update
heiwa update --channel stable
```

## Scope Guardrails (what's IN, what's OUT)

IN — part of `heiwa`:

- verb dispatch, REPL, routing decisions, approval plumbing
- session + memory + trace state, local-first
- provider-surface management (link, unlink, test)
- local cockpit HTTP server
- scheduled routine execution
- update + install bootstrapping
- plugin/skill install via `gh:` scheme

OUT — not `heiwa`'s job:

- inference itself (providers own it)
- long-running background daemons beyond the scheduler (use the OS, not a Heiwa daemon)
- multi-tenant auth, account systems, billing (Heiwa has no hosted plane)
- shipping prompts that imply Heiwa speaks for the operator (the operator speaks; Heiwa routes)
- any verb that requires a remote Heiwa-operated server to function

## Migration From Current State

Today:

- `heiwa` the binary is partial; multiple entry points exist (`heiwa_shell`, `heiwa_cli`, `~/.heiwa/bin/heiwa-route`)
- verbs are scattered across scripts

Direction:

1. Consolidate entry points into a single `heiwa` binary (Rust preferred for the dispatcher; Python subsystems remain callable).
2. Preserve `~/.heiwa/bin/heiwa-route` as a compatibility shim that forwards to `heiwa route` until external references are updated.
3. Land verbs in priority order: `run`, `repl`, `providers`, `route`, `app`, `install`, `update`, `doctor`, `memory`, `trace`, `agent`, `cron`, `loop`, `approvals`, `config`, `plugin`, `browse`.
4. Track verb maturity in a single JSON the cockpit can read, same pattern as `providers.json`.

## What This Spec Does Not Decide

- implementation language for each subsystem (Rust vs Python by subsystem — stays open per HEIWA.md)
- wire format between `heiwa` and `heiwa_core` (HTTP vs WS vs UDS — implementation detail)
- per-verb prompt templates (product content, not CLI shape)
- cockpit route tree (covered in the Vite+Solid app, not here)
