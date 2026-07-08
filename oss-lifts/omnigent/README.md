# OmniGent -> Heiwa multiplexer lift

Source clone: `~/oss-repos/omnigent`
Upstream: `https://github.com/omnigent-ai/omnigent`
Docs inspected: `https://omnigent.ai/quickstart/install`
Upstream commit: `c975f629014ea1f4980784707e354baf61466053` (`2026-06-20`, `test: fix mock /chat/completions tool_calls; un-quarantine yaml_agent_with_tools[pi] (#807) (#878)`)
License: Apache-2.0. Pattern lift allowed; copied code needs attribution/NOTICE review.
Local prerequisite check: `tmux 3.6a` at `/opt/homebrew/bin/tmux`.

## Verdict

Useful for Heiwa, but not as a dependency.

OmniGent is alpha, Python-first, server/web-collab-first, and provider-harness-first. Heiwa's center of gravity stays the installed Rust `heiwa` runtime plus `~/.heiwa` local truth. Treat OmniGent as a reference implementation for the missing **local multiplexer** layer: tmux-backed worker/session terminals, attach/read-only views, lifecycle cleanup, and terminal capability declarations.

## Extracted value

1. **tmux as required substrate**
   - Official install docs require Python 3.12+, Node 22/npm, and `tmux`.
   - OmniGent native Claude/Codex wrappers rely on tmux terminals.
   - Heiwa should make `tmux` a first-class `heiwa doctor --ai-ops` prerequisite for the multiplexer plane, not a hidden best-effort dependency.

2. **Conversation-scoped terminal registry**
   - Upstream key: `(conversation_id, terminal_name, session_key)`.
   - Multiple sessions of the same terminal name can run in parallel.
   - Registry owns launch, list, send, read, close, transfer, conversation cleanup, and server shutdown.
   - Heiwa translation: `heiwa_terminal::Registry` keyed by `(session_id, terminal_name, instance_id)` under the current Heiwa session/evidence model.

3. **Private tmux server per terminal**
   - Upstream creates one isolated tmux socket per terminal instance.
   - It disables inherited tmux config, locks down prefix/window creation, controls history size, strips dangerous env vars, and kills orphaned tmux servers whose owner PID died.
   - Heiwa translation: no global user tmux server for agent work. Use private sockets under `~/.heiwa/state/terminals/<session>/<instance>/tmux.sock`.

4. **Attach bridge contract**
   - Upstream exposes browser attach through a PTY/WebSocket bridge around `tmux attach`.
   - Read-only attach uses both `tmux attach -r` and app-level dropped input.
   - Interactive write attach requires owner-level permission because raw PTY bytes carry no sender identity.
   - Heiwa translation: app/cockpit can render terminals, but write input must be owner-scoped and receipt-bearing; non-owner/future shared views read-only by default.

5. **Agent-declared terminal capabilities**
   - Upstream `terminals:` YAML declares named terminals with command, args, env, cwd override policy, sandbox override policy, scrollback, and tmux flags.
   - Tool surface appears only when terminals are declared: `sys_terminal_launch`, `send`, `read`, `list`, `close`.
   - Heiwa translation: provider/worker manifests declare terminal leases. Runtime grants `terminal.launch/read/send/close` only through DREX lease + approval policy.

6. **Policy/approval event shape**
   - OmniGent's ASK path emits MCP-shaped `response.elicitation_request` and parks until approval, decline, cancel, malformed result, or timeout.
   - Heiwa already has approvals/dispatch. Lift only the round-trip shape and fail-closed timeout semantics where it improves current dispatch approvals.

## Not useful / do not copy

- Do not import OmniGent as Heiwa runtime dependency.
- Do not copy Python server/store architecture into Heiwa's Rust runtime.
- Do not adopt OmniGent as source of truth for provider auth, model routing, or product identity.
- Do not expose tmux socket paths to pane processes.
- Do not let user tmux config affect managed agent terminals.
- Do not treat Debby/Polly prompts as Heiwa doctrine. They are examples, not authority.

## Heiwa translation

Target product noun: **local multiplexer**.

Heiwa requirement:

> Heiwa is one operator thread over many live workers. Each worker can own durable terminal/process state, be attached from the app or CLI, be paused/resumed/cancelled, and emit receipts. tmux is the local terminal substrate.

First implementation should be Rust-native and small:

- New crate: `crates/heiwa_terminal`
  - `TerminalSpec`: `name`, `command`, `args`, `env`, `cwd_root`, `allow_cwd_override`, `scrollback`.
  - `TerminalId`: `session_id`, `name`, `instance_id`.
  - `TerminalRegistry`: `launch`, `list`, `send_text`, `send_keys`, `read`, `close`, `cleanup_session`, `shutdown`.
  - tmux private socket root under `~/.heiwa/state/terminals/`.
  - orphan sweep by owner PID marker.
- Shell command: `heiwa terminal ...`
  - `launch <name> --session <id> [--cwd ...]`
  - `list [--session <id>]`
  - `send <terminal-id> --text ... [--keys Enter]`
  - `read <terminal-id> [--scrollback N]`
  - `close <terminal-id>`
- Doctor: `heiwa doctor --ai-ops`
  - hard-check `tmux` binary and version.
  - report missing tmux as multiplexer-blocking.
- Evidence:
  - emit local receipts for launch/send/read/close with session id, terminal id, command, cwd, status, and error.
  - never record raw terminal body by default; record bounded preview/hash unless explicit diagnostic mode.

## First slice

Implement only deterministic CLI/runtime multiplexer. No web attach yet.

Implementation plan: `docs/superpowers/plans/2026-06-20-heiwa-tmux-multiplexer.md`

Files likely touched:

- Create `crates/heiwa_terminal/Cargo.toml`
- Create `crates/heiwa_terminal/src/lib.rs`
- Modify root `Cargo.toml`
- Modify `apps/heiwa_shell/Cargo.toml`
- Create `apps/heiwa_shell/src/cmd/terminal.rs`
- Modify `apps/heiwa_shell/src/cmd/mod.rs`
- Modify `apps/heiwa_shell/src/cli.rs`
- Modify `apps/heiwa_shell/src/main.rs`
- Add `crates/heiwa_terminal/tests/registry.rs`
- Add `apps/heiwa_shell/tests/terminal.rs`

Acceptance:

- `tmux` missing returns a clear typed error.
- `launch` starts private tmux socket under a temp/state root and does not use `~/.tmux.conf`.
- `send` + `read` round trip against `bash`.
- `list` shows running state and socket path.
- `close` is idempotent.
- session cleanup closes all terminals under that session.
- orphan sweep kills stale tmux server only when owner PID is gone.

Test commands:

- `cargo test -p heiwa_terminal`
- `cargo test -p heiwa_shell terminal`
- `cargo build -p heiwa_shell`

Manual smoke:

```bash
heiwa terminal launch shell --session local-smoke --cwd /Users/dmcgregsauce/heiwa-universe
heiwa terminal send shell:local-smoke --text 'printf hello' --keys Enter
heiwa terminal read shell:local-smoke
heiwa terminal close shell:local-smoke
```

## Later slices

1. App attach bridge:
   - WebSocket or SSE+input route over `tmux attach`.
   - Read-only attach first.
   - Owner-write attach later with explicit attribution/receipt.

2. Worker integration:
   - Provider adapters can request terminal leases.
   - Claude/Codex/Gemini/Antigravity/Ollama workers map to terminal specs.
   - Worker status derives from tmux/process state plus receipts.

3. Multiplexer UI:
   - Heiwa.app shows sessions, workers, terminal previews, log tails, pause/resume/cancel.
   - Chat remains projection over event log, not source of truth.

4. Policy integration:
   - Terminal launch/send/close pass through DREX lease and approval gates.
   - Remote/shared attach defaults read-only.

## Personal ops use

High ROI:

- stable local tmux sessions for long-running Heiwa work, logs, repo tests, Calendar/mail connectors, and background monitors;
- phone/browser read-only observation later;
- fewer lost agent runs when terminal/UI disconnects;
- cleaner "what is running right now?" view.

Low ROI:

- installing and using OmniGent directly as Devon's main ops layer. It overlaps Heiwa too much and would create another routing/auth/session authority.
