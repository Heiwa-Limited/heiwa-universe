# Heiwa App Foundation

Date: 2026-05-26

## Decision

Use **Tauri 2 + existing Solid/Vite cockpit + `heiwa_shell` local API/runtime**.

Do not choose Electron for the primary app. Do not move local owner state into
the frontend. Do not make `apps/heiwa_core` read Devon-local files.
Do not claim this choice is peer-validated by OpenHuman. It is a Heiwa
toolchain decision that must be proved against the local cockpit.

Compression:

> Tauri displays. `heiwa` thinks, routes, runs, records, and serves.

## Why

- Tauri matches Heiwa's Rust-first runtime and should keep desktop bundle weight low.
- Existing cockpit is already Solid/Vite/TypeScript and can be wrapped without a UI rewrite.
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
- DREX routes work to local models, provider CLIs, API providers, workers, or remote machines.
- `~/.heiwa/` stores machine truth, approvals, workers, traces, receipts, and local state.
- STDB syncs adjudication/evidence when enabled, but users do not operate STDB directly.

Desktop:

- `Heiwa.app` is a Tauri wrapper over the cockpit.
- Cockpit consumes local `/api/v1/*` and later `/ws/v1/*`.
- Tauri commands are for OS integration only: tray, notifications, secure storage, file picker, login items, and local process supervision.
- UI does not own policy or state.

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
2. SSE/WebSocket event stream for run/tool/worker/receipt events.
3. Cockpit Today/Approvals surface over those local APIs.
4. Tauri wrapper around existing cockpit and runtime start/attach.
5. Local multiplexer: sessions, workers, PTY/log tail, pause/resume.
6. Machine registry and remote attach.
7. Connector sync lane: auth, list, one bounded action, evidence receipt, revoke.
8. Compression and learning loop: source-chunk compression, skill/procedure evolution, review gates.
9. Memory tree and Markdown export/import.

## Sources

- Tauri: https://tauri.app/
- Electron: https://www.electronjs.org/docs/latest/why-electron
- OpenHuman: https://github.com/tinyhumansai/openhuman
- Hermes Agent: https://github.com/NousResearch/hermes-agent
- Claude Desktop MCP: https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop
- Claude Code Desktop: https://code.claude.com/docs/en/desktop
- Codex CLI: https://developers.openai.com/codex/cli
- Gemini CLI: https://developers.google.com/gemini-code-assist/docs/gemini-cli
