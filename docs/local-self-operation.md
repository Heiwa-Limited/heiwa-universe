# Local Self-Operation

This is the runtime contract for Heiwa on Devon's MacBook first. The same
contract must scale to each enrolled user machine without changing the product
model.

The goal is simple: the installed `heiwa` runtime should authenticate provider
CLIs through their owner-managed configs, read/write local state under
`~/.heiwa`, expose the cockpit on localhost, and sync evidence to SpacetimeDB
only when that path is configured.

## Required Local Inputs

| Input | Purpose |
| --- | --- |
| `~/.heiwa/config.toml` | Runtime configuration |
| `~/.heiwa/accounts.json` | Provider/account registry |
| `~/.heiwa/machine.json` | Local machine identity and capability manifest |
| `~/.heiwa/state/` | Local runtime state, approvals, worker heartbeats |
| `~/.claude/`, `~/.codex/`, `~/.gemini/` | Provider-owned auth and hook posture |
| `spacetime login` shell identity | Optional SpacetimeDB Maincloud sync/adjudication auth; canonical publisher/operator path |
| `STDB_TOKEN` | Legacy/compat SpacetimeDB token material only; not the preferred Heiwa operator auth boundary |
| `CLOUDFLARE_API_TOKEN` | Optional edge work only; not needed for local user functionality |

## Boot Contract

`heiwa app start --port 7474` must:

1. Serve the cockpit and local API on `127.0.0.1`.
2. Report health at `/status/health`.
3. Write local app worker heartbeats under `~/.heiwa/state`.
4. Report provider, route, approval, worker, and hook posture without mutating provider-owned configs.
5. Keep running without public DNS, Cloudflare auth, or SpacetimeDB connectivity.
6. Refresh `~/.heiwa/machine.json` with current host, OS, arch, install path, runtime version, and capability probes.
7. Adapt worker concurrency, polling cadence, and local-model use to machine load, battery, thermal state, and available runtimes.
8. Surface pending update or restart requirements without interrupting active work.

## Install and Update Authority

GitHub and Cloudflare form the public install source, but they do not have the
same authority.

| Surface | Authority |
| --- | --- |
| GitHub repository | Canonical source code, tags, CI evidence, release artifacts, checksums, and install scripts |
| GitHub Releases | Canonical binary/archive distribution and version provenance |
| Cloudflare | Public edge, docs, install landing pages, update manifest cache, status, and future remote attach |
| Local machine | Installed binary, local config, provider auth, local state, and user-approved side effects |

Cloudflare may front or cache install/update material, but it must point back to
GitHub release identity and checksums. Cloudflare must not become a second
source of binary truth.

Under the local-first emergency bypass posture, local checkout source promotion (`heiwa app update --source checkout`) is the authoritative install and update path on Devon's MacBook. The MacBook operates directly from locally-verified sandbox artifacts rather than waiting for GitHub Releases.

`heiwa app update --dry-run` is the safe probe for the installed runtime and
defaults to GitHub Releases. It should report:

- installed version and path
- target version, channel, and release URL
- release commit or tag
- checksum/signature status when available
- whether restart is needed
- whether active tasks block restart

The runtime should prompt for update/restart when a newer compatible release is
detected, when cockpit assets are newer than the running server, or when a
schema/runtime boundary requires restart.

## Restart and Update Contract

Restart is an operator-visible state transition, not a silent side effect.

Default behavior:

1. Detect update or restart requirement.
2. Classify active work as `none`, `pausable`, or `blocking`.
3. Prompt the operator with target version, source, expected downtime, active tasks, and rollback path.
4. Apply update/restart only after approval.
5. Emit an evidence receipt with before/after versions and task handling.

Optional auto-restart is allowed only when explicitly enabled and one of these
conditions holds:

- no active tasks, no pending approvals, no external side effects in flight
- all active tasks are paused, leased work is checkpointed, and traces/events are flushed

Auto-restart must not run while a provider subprocess, file mutation, network
mutation, payment, booking, message send, or credential operation is in flight.
Those cases require an approval prompt.

Pause-before-restart must:

1. Stop accepting new work.
2. Mark active tasks as paused with restart reason.
3. Close or renew leases deterministically.
4. Flush `~/.heiwa/state`, traces, logs, and evidence receipts.
5. Restart the runtime.
6. Rehydrate machine state and resume only tasks whose leases and approval policy still allow continuation.

## Machine Initialization and Adaptation

Each machine initializes as a local Heiwa node with its own capabilities. Heiwa
must assume N user machines over time, not one hardcoded owner path.

On first boot or install, the runtime should:

1. Create or refresh `~/.heiwa/machine.json`.
2. Record stable machine id, hostname, OS, arch, CPU/GPU class, memory, battery/thermal availability, install path, and runtime channel.
3. Discover local providers and CLIs without mutating provider-owned configs.
4. Discover local model runtimes such as Ollama.
5. Register or sync machine identity with SpacetimeDB only when configured.
6. Write a boot receipt under local evidence state.

Adaptation rules:

- Battery or thermal pressure reduces background polling and pauses non-urgent work.
- Low memory or CPU load pressure reduces concurrency before degrading UX.
- Machines with strong local models should take cheap sovereign work first.
- Machines without local models should route through approved provider lanes.
- Machine-specific provider auth stays local and provider-owned.
- Cross-machine truth is synchronized through evidence and machine identity, not by sharing raw secrets.

## Agentic Runtime Workflow

Use this workflow when an AI agent is developing, testing, or operating Heiwa.
The goal is to prove the current runtime, avoid stale localhost processes, and
leave no temporary process or file behind.

### 1. Understand before acting

Read in this order before architecture or runtime changes:

1. [`HEIWA.md`](../HEIWA.md) for canonical product truth.
2. [`AGENTS.md`](../AGENTS.md) for repo-specific agent rules.
3. This file for local boot, stop, and verification rules.

Classify the task as **Intake**, **Execution**, **Evidence**, or
out-of-scope before editing. If the work does not advance one of those planes,
defer it.

### 2. Probe without mutating

Start every runtime task with no-side-effect probes:

```bash
heiwa app update --dry-run
heiwa app runtime status --json
heiwa providers
```

When working from the checkout instead of the installed binary, prefer:

```bash
cargo run -q -p heiwa-shell --bin heiwa -- app runtime status --json
cargo run -q -p heiwa-shell --bin heiwa -- app update --source checkout --dry-run
```

Check the reported `cli_path`, `state_dir`, `local_app.url`, and
`local_app.reachable`. Also check update/restart hints when present. A reachable
app only proves that something is listening; it does not prove that the listener
is the code you just changed.

### 3. Avoid stale runtimes

Treat port `7474` as the installed product runtime. Do not assume it reflects
the current checkout after code edits.

For development verification, start a current checkout runtime on a temporary
alternate port:

```bash
cargo run -q -p heiwa-shell --bin heiwa -- app start --port 7475 --no-open
```

Then probe that same port:

```bash
curl -fsS http://127.0.0.1:7475/status/health
curl -fsS http://127.0.0.1:7475/api/v1/session
```

If a new API endpoint returns `index.html`, the request fell through to static
SPA serving. Assume you are probing the wrong runtime, an old runtime, or an
unimplemented route until proven otherwise.

Only run `heiwa app update` when the operator explicitly wants the installed
runtime changed. `--dry-run` is the default probe. Use
`heiwa app update --source checkout` only for developer reinstall from the
current checkout.

### 4. Start safely

Before starting a long-running runtime, decide:

- which port it owns
- whether it is installed-product verification or checkout verification
- what command will stop it
- what files, if any, will be created for probes
- whether restart/update prompts should be shown, deferred, or ignored for this verification

Prefer `--no-open` for agent verification so the browser is not disturbed.

### 5. Use the runtime

Use the local API and cockpit against the same port you started. Keep evidence
local and concrete:

```bash
curl -fsS http://127.0.0.1:7475/status/health
curl -fsS http://127.0.0.1:7475/api/v1/runtime/snapshot
curl -fsS http://127.0.0.1:7475/api/v1/inbox
curl -fsS http://127.0.0.1:7475/api/v1/history
```

Do not fabricate cockpit rows. If the UI needs data, wire it to existing
`~/.heiwa/state` truth or add a clearly scoped read model with tests.

### 6. Stop what you started

Every agent-started runtime must be stopped before final reporting unless the
operator explicitly asks to keep it running.

Preferred stop order:

1. Send normal interrupt or SIGTERM to the exact process you started.
2. Confirm the command prints its shutdown line or the port stops responding.
3. Do not kill unrelated Heiwa processes on other ports unless the operator
   asked for that cleanup.

If sandbox policy blocks stopping a process, request escalation for the exact
PID and explain that it is the temporary runtime started for verification.

### 7. Clean as you go

Clean up temporary verification artifacts before final reporting:

- temporary JSON probe files under `/private/tmp`
- ad hoc fixture directories created by tests
- one-off logs created only for the current verification
- temporary alternate-port runtime processes

Do not delete durable runtime truth under `~/.heiwa/state`,
`~/.heiwa/sessions`, `~/.heiwa/logs`, or evidence directories unless the
operator explicitly requests it.

Before final reporting, run:

```bash
git status --porcelain=v1 -uall
```

Report remaining dirty files honestly, separating agent changes from
pre-existing or peer-agent changes.

## Model Tier Matrix

| Lane | Primary | Secondary | Notes |
| --- | --- | --- | --- |
| Routine chat/status/audit | `ollama/*` where sufficient | Gemini CLI / Antigravity | Cheapest acceptable route first |
| Build/code | Codex CLI | Claude Code, Gemini CLI, Ollama coding model | Provider CLIs own their auth and quota semantics |
| Research/long context | Gemini CLI | Antigravity, Claude Code | Escalate only when local context is insufficient |
| Review/strategy | Claude Code / Gemini | Codex | Use premium lanes intentionally |
| Sovereign work | local `ollama/*` tiers | none | Local-only providers only |
| Embeddings | `ollama/qwen3-embedding:0.6b` | none | Local runtime default |

## Verification

```bash
heiwa app update --dry-run
heiwa app runtime status --json
heiwa providers
curl -fsS http://127.0.0.1:7474/status/health
```

The runtime is not ready for public access until the localhost checks pass and
Cloudflare is explicitly re-enabled with fresh targets.
