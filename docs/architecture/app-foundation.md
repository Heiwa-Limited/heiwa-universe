# Heiwa App Foundation

Date: 2026-07-19

## Decision

Use **Tauri 2 + a minimal TypeScript/Vite desktop shell + `heiwa_shell`
local API/runtime**.

Do not choose Electron for the primary app. Do not move local owner state into
the frontend. Do not make `apps/heiwa_core` read Devon-local files.
Do not canonize Solid, router libraries, component libraries, or dashboard kits
as product dependencies. The current web cockpit can remain a support surface,
but the app stack is runtime-first and multiplexer-first.
Do not claim this choice is peer-validated by OpenHuman. It is a Heiwa
toolchain decision that must be proved against local runtime use.

Compression:

> Tauri displays. tmux hosts panes. `heiwa` thinks, routes, runs, records, and serves.

## Stack Critique

Keep:

- Rust runtime authority in `heiwa_shell`, DREX, receipts, sessions, providers,
  approvals, and local state.
- Tauri 2 as the native app wrapper over the same local API/runtime.
- TypeScript/Vite as a small app build layer, not as the owner of policy or state.
- tmux as the local multiplexer substrate for panes, windows, worker terminals,
  attach/read/write boundaries, and future remote attach.
- Local append-only JSONL as canonical evidence truth, with Lance as a
  rebuildable local recall projection. GitHub evidence sync remains planned,
  local-first, and blocked behind an explicit redaction/privacy boundary.

Drop or defer:

- extra app UI libraries until a concrete pane/window API proves the need
- feature-specific mini-apps that make Calendar, Mail, Finance, Social, AI, and
  Files feel like separate products
- dashboard-first routing where users manually choose models, tools, or backends
- direct frontend reads of `~/.heiwa` state
- hosted-control-plane assumptions for local user work

Selected app shape:

- default Home is the pinned ops board: visible terminal instances, workers,
  sub-app servers, approvals, receipts, and live run state
- left dock is secondary navigation: hover previews are useful, but Home must
  show the active panes without forcing hover
- primary work happens in pinned multiplexer windows/panes: conversation,
  workers, terminal instances, calendar/mail/file context, approvals, receipts,
  and ops queues
- feature surfaces are connected sub-apps with their own agent profile, tools,
  skills, and personalization, but they still route through Heiwa's runtime
  brain/evidence policy
- `herdr` plus Deno are valid spike/runtime adapters for pane visibility and
  sub-app servers; the product path must still converge on Heiwa-owned runtime
  contracts rather than a loose collection of servers

## Why

- Tauri matches Heiwa's Rust-first runtime and should keep desktop bundle weight low.
- The desktop client can stay dependency-light: Tauri API plus code-native
  TypeScript views, with Vite/TypeScript as dev/build tools.
- Existing `heiwa app start` already binds localhost, serves cockpit assets, writes worker heartbeats, and exposes `/api/v1/*`.
- OpenHuman is adjacent evidence: Rust desktop app, Tauri/CEF shell, local memory, managed OAuth/integration path, UI-first onboarding.
- Hermes is adjacent evidence: terminal-first personal agent, skills, memory, messaging gateway, cron, MCP, provider switching, and remote/server execution. It is not a worker mesh reference.
- Claude Desktop, Claude Code, Codex, and Gemini CLI validate the display/routing pattern: conversation plus visible tool events, approvals, sessions, file/artifact diffs, command output, MCP/tool config, and sidebars or terminal lanes for parallel work.

## Peer Bar

OpenHuman:

- UI-first desktop app.
- Local Memory Tree plus editable Markdown/Obsidian-style vault.
- Managed default services for account sign-in, model routing, search proxying,
  OAuth, and Composio-backed integrations.
- 118+ integrations claim, 20-minute auto-fetch, TokenJuice compression, voice,
  and Google Meet agent.
- Uses Rust and vendored Tauri/CEF sources. This supports "Rust desktop is viable";
  it does not prove plain Tauri 2 WebView is enough for Heiwa.

Hermes:

- Terminal-first, self-improving agent.
- Python, server/VPS/GPU/serverless friendly.
- Skills, FTS5 memory/search, Honcho user modeling, cron delivery, messaging gateways, MCP, provider routing.
- Seven terminal backends are execution environments for the agent shell, not a cooperating-agent mesh.
- UI is not the product moat; durable execution loop is.

Claude Desktop:

- Human display surface for model/tool work.
- Connectors/MCP UI.
- Computer-use approval boundary.
- Multiple sessions and sidebar work management.

Claude Code / Codex / Gemini CLI:

- Workspace-bound agent shells.
- Local file diffs, command output, tests, artifacts, screenshots.
- MCP/tool config, local/remote execution modes, and provider-owned auth/quota semantics.
- Gemini CLI specifically matters as open-source terminal agent with ReAct loop and local/remote MCP server support.
- Provider owns inference; Heiwa owns coordination surface.

## Heiwa Shape

Runtime:

- `heiwa_shell` is the local owner runtime and API host.
- DREX plans every model call against its own capabilities, privacy/risk,
  quality floor, success floor, and marginal-cost budget. Only candidates that
  clear those gates compete on cost.
- `~/.heiwa/` stores machine truth, approvals, workers, traces, receipts, and local state.
- The append-only JSONL journal under `~/.heiwa/evidence/` is durable truth.
  Lance and SQLite/FTS read models are derived and rebuildable; GitHub evidence
  sync is planned and redaction-gated, not live.
- `heiwa_session::OperatorSessionService` is the sole domain writer for
  `operator_events.jsonl`. `heiwa_evidence` owns dumb append/replay framing,
  cursor validation, locking, fsync, and sensitive-material rejection; command
  handlers, clients, and projections do not append operator events directly.

Desktop:

- `Heiwa.app` is a Tauri wrapper over the app shell.
- The app shell consumes authenticated local `/api/v1/operator/*` and
  `/ws/v1/operator` for operator state alongside narrower runtime read models.
  Localhost is a transport boundary, not an authentication boundary.
- The native Tauri bridge reads local runtime auth, restricts transport to the
  configured `127.0.0.1` runtime port, and injects bearer authentication below
  the renderer. The TypeScript renderer never owns or persists the machine
  token.
- Tauri commands stay narrow: authenticated loopback transport plus OS
  integration such as tray, notifications, secure storage, file picker, login
  items, and local process supervision.
- UI does not own policy or state.
- Home is the pinned ops view: live terminal/herd panes first, then compact
  widgets for sub-app servers, agent skills/tools, personalization, approvals,
  receipts, and provider posture.
- Feature icons are dock entries with hover previews, but they are not the main
  visibility model. Opening a feature focuses its pinned sub-app/window.
- Calendar, Mail, Finance, Social, AI, Files, Browser, and terminals are
  sub-app panes inside the multiplexer model, not separate app silos.
- Each sub-app has an app-local agent profile: relevant skills, allowed tools,
  risk posture, and personalization rules for the operator's current context.
- Deno/herdr can host spike sub-app servers and pane APIs today. The app should
  read those surfaces when available, while keeping Rust runtime authority,
  approvals, and evidence as the durable contract.

Packaged app format:

- Browser preview is only a development convenience. Product use is packaged
  `Heiwa.app` with bundled assets and Tauri commands over local runtime APIs.
- The app should not rely on browser-open tabs for core ops. It should call
  native commands for local-only bridges such as herd/pane state, and those
  commands should read the Rust runtime API, a packaged Deno sidecar, or
  provider-owned local CLIs.
- Deno belongs as a lightweight packaged sidecar/server lane for sub-apps and
  spike iteration, especially where TypeScript-native app logic is useful.
  It must not become a second policy/evidence authority.
- `herdr`/Deno pane visibility can feed the app today. The durable target is a
  Heiwa terminal daemon contract that exposes the same shape to app, TUI, and
  REPL.

Terminal surfaces:

- `heiwa` REPL remains the fastest operator surface and must share session,
  routing, approval, and receipt state with the app.
- `heiwa_tui` is the terminal-native visual cockpit for the same event stream:
  transcript, inspector, composer, status, approvals, workers, and eventually
  pinned panes.
- `heiwa_session` is the daemon/PTY foundation. Its socket/daemon path should
  evolve into the local terminal daemon behind pane state, attach, send/read,
  pause/resume, and receipt events.
- App, TUI, and REPL are three displays over one state machine. They must not
  fork write paths or invent separate automation authority.

## Operator Stream Contract

The live operator stream is the current conversation/execution contract for the
Desktop and authenticated HTTP/WebSocket surfaces. The `/api/v1/repl`
compatibility routes submit through the same operator runner. Full interactive
CLI and TUI consumption of this stream remains a convergence target:

- durable, totally ordered domain events live in `operator_events.jsonl`
- `OperatorSessionService` admits turns idempotently by
  `client_request_id`, folds thread/turn state, and is the only domain append
  authority
- app startup acquires an exclusive, zero-content app-runtime lease for the
  configured evidence root before recovery, heartbeat, or API service. Every
  session service that mutates the operator stream also holds a shared activity
  lease; restart recovery requires exclusive activity ownership and fails
  closed while a CLI, REPL, loop, or other session writer is live. Isolated
  evidence roots remain independent
- authenticated HTTP provides thread creation, replay, turn submission, and
  cancellation; authenticated WebSocket provides cursor-based replay plus live
  durable and transient frames
- opaque cursors are versioned and bound to one stream lineage. Unknown,
  replaced, truncated, or non-boundary cursors return structured
  `invalid_cursor`; clients clear only disposable projections and replay the
  thread from the beginning
- event-id deduplication makes replay safe, while assistant deltas remain
  transient and only completion events become durable transcript truth
- restart recovery appends one terminal `turn_interrupted` event for every
  nonterminal turn (`RUNTIME_RESTART`, or `OPERATOR_CANCELLED` when cancellation
  was already pending). No in-memory liveness is silently resurrected

The Desktop reducer is a disposable projection of this contract. The current
authenticated API and REPL compatibility routes share its runtime/session state
machine; interactive CLI and future TUI views must converge without adding a
second write path.

## Per-Call Routing And Cost Truth

Routing is per model call, not permanently fixed per thread, turn, or worker.
`apps/heiwa_shell/src/model_calls.rs` is the provider-invocation boundary: DREX
filters candidates against the call's required capabilities, locality,
privacy/risk, quality floor, success floor, allow/exclude policy, and remaining
budget, then selects the cheapest eligible candidate. Availability, auth,
quota, timeout, and provider failures are recorded before DREX replans the next
attempt with failed candidates excluded.

`route_planned` carries the selected candidate's cost-truth class when a
candidate exists. `route_completed` carries actual completion cost truth, while
`route_failed` carries the failed attempt's available cost truth.
`route_attempted` records invocation identity only; it does not claim spend,
and a no-selection plan has no selected cost truth. Current cost-truth classes
are:

| Class                   | Meaning                                                                 |
| ----------------------- | ----------------------------------------------------------------------- |
| `local_zero_cost`       | No marginal provider charge for the local call; not a claim of zero hardware cost |
| `target_only`           | A configured target/budget value, not provider-reported spend          |
| `proxy_estimate`        | An estimate derived from known pricing or a comparable pricing proxy   |
| `exact_provider_report` | The connected provider reported the call's actual usage cost           |
| `cannot_confirm`        | Heiwa has no defensible marginal-cost number and does not invent one    |

The policy is **cheapest above the per-call quality floor**, not cheapest-first.
That quality floor is the value control that lets a later call in the same turn
escalate to a stronger model while routine calls remain local or inexpensive.

Remote/N machines:

- Each machine runs a local `heiwa` node.
- Each machine has `~/.heiwa/machine.json`, capability manifest, local provider auth, and local receipts.
- Remote attach uses machine identity plus authenticated tunnel/relay later.
- Secrets stay local to each machine.
- App shows machine switcher, worker lanes, approvals, receipts, and health.

## AI Output Display Contract

Display machine work as human-readable events:

- `thought_status`: short status only, no hidden chain-of-thought dump.
- `tool_call`: tool, target, mode, risk, status.
- `approval_request`: target, payload, cost/risk, expected receipt.
- `worker_spawned`: provider, model/local runtime, machine, lease, budget.
- `artifact`: file, diff, report, image, receipt.
- `test_result`: command, pass/fail, output summary.
- `receipt`: source refs, evidence refs, timestamp, actor.
- `blocker`: exact missing auth, quota, capability, permission, or data.

The app should stream events first. Chat summary is a projection of the event
log, not the source of truth.

## Build Order

1. Local API read models: Today, Freshness, ApprovalSummary.
2. Local multiplexer: tmux/herdr-backed sessions, pinned panes, workers,
   PTY/log tail, pause/resume, and receipt hooks.
3. Home pinned ops board fed by herd/terminal state, approvals, receipts,
   runtime status, and sub-app server status.
4. Sub-app server contract: Calendar, Mail, Finance, Social, AI, Files, Browser
   expose skills, allowed tools, personalization, and evidence hooks.
5. Native app bridge: Tauri commands for runtime health/API, herd/pane state,
   packaged Deno sidecars, and terminal daemon attach/read/send.
6. Extend the authenticated operator WebSocket contract to remaining
   terminal/sub-app event families without creating another state machine.
7. TUI/REPL parity: same session, approval, receipt, and terminal daemon state
   visible in `heiwa shell`, `session attach`, and `heiwa_tui`.
8. Connector sync lane: auth, list, one bounded action, evidence receipt, revoke.
9. Machine registry and remote attach.
10. Compression and learning loop: source-chunk compression, skill/procedure evolution, review gates.
11. Memory tree and Markdown export/import.

## Sources

- Tauri: https://tauri.app/
- Electron: https://www.electronjs.org/docs/latest/why-electron
- OpenHuman: https://github.com/tinyhumansai/openhuman
- Hermes Agent: https://github.com/NousResearch/hermes-agent
- Claude Desktop MCP: https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop
- Claude Code Desktop: https://code.claude.com/docs/en/desktop
- Codex CLI: https://developers.openai.com/codex/cli
- Gemini CLI: https://developers.google.com/gemini-code-assist/docs/gemini-cli
