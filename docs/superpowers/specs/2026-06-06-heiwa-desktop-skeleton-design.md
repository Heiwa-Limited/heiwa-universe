# Heiwa Desktop Skeleton (S0) — Design

**Date:** 2026-06-06
**Status:** Revision candidate — user review required before implementation plan
**Scope:** Foundation slice (S0) of the Heiwa Desktop App. Shell + runtime
bridge + navigation only. Verticals (Chat, Calendar, Trading, Agents,
Dashboard contents) are later slices, each with its own spec.

## Goal

Stand up a clean, high-quality Tauri 2 desktop app that all current and future
Heiwa surfaces can ride on. The skeleton must prove — end to end — that the
desktop shell wires to the local runtime cleanly and efficiently, so future
verticals plug in with near-zero plumbing.

This is a UI/UX consolidation foundation: reverse-engineer the runtime's
existing contract and build a clean client over it, rather than carrying forward
the mid-quality cockpit code.

## Non-goals (explicitly out of S0)

- Real Chat loop, WS streaming, session state.
- Real data rendering for Calendar/Trading/Agents/Dashboard beyond placeholders
  plus the single canary.
- Porting cockpit code/components.
- Packaging/installer work that replaces the HOME-local launcher bundle.
- Auth/login UI.
- Choosing or building a new Python background server.
- New memory, context, or dashboard storage mechanics.
- Direct app-to-SQLite or app-to-SpacetimeDB writes from the renderer.
- Any backend/runtime changes (e.g. a trading endpoint). The skeleton consumes
  the contract as-is.

## Context

### Canon vs decision point

- **Canonical app foundation:** Tauri 2 remains the native wrapper direction:
  it fits Rust runtime authority and can host the same local client contract
  without moving control into a hosted dashboard. Do not cite OpenHuman as
  proof — it uses vendored Tauri/CEF.
- **Renderer choice is not canonical:** the current cockpit is Solid/Vite, and
  `HEIWA.md` currently names that pairing, but this spec must not treat
  Solid/Vite as settled if Devon wants to revisit the desktop/dashboard stack.
  Tauri is frontend-agnostic enough that S0 can use Solid/Vite, vanilla
  TypeScript, Svelte, React, Leptos/Yew, or another renderer. The implementation
  plan must lock one deliberately.
- **Runtime authority:** `Heiwa.app` is the display/input shell over the runtime;
  the native wrapper replaces today's launcher bundle *"without changing runtime
  authority"* (`HEIWA.md:140`, installed at `~/.heiwa/app/Heiwa.app`).
- **Bridge target:** the runtime app server binds `127.0.0.1:7474`
  (`apps/heiwa_shell/src/cmd/app.rs:21`, `DEFAULT_PORT = 7474`) and serves a
  JSON `/api/v1/*` surface over plain HTTP (a hand-rolled method+path match loop,
  not an axum router). Local-only; not network-exposed.

### Reverse-engineered `/api/v1` contract (the thing we wire to)

Read endpoints today (`apps/heiwa_shell/src/cmd/app.rs`):

| Endpoint | Feeds (future vertical) |
| --- | --- |
| `GET /api/v1/runtime/snapshot` | shell status canary |
| `GET /api/v1/resource` | Dashboard → model scorecard (machine resources) |
| `GET /api/v1/providers` | Dashboard → 3rd-party accounts |
| `GET /api/v1/routes`, `/rate-groups`, `/capabilities` | Dashboard → DREX routing |
| `GET /api/v1/memory` | Dashboard → memory access (currently empty read model) |
| `GET /api/v1/agents`, `/approvals`, `/approvals/summary`, `/missions` | Agents & sandboxes |
| `GET /api/v1/life/today`, `/life/freshness` | Calendar / Today |
| `GET /api/v1/goals`, `/history`, `/traces`, `/hooks`, `/crons`, `/inbox`, `/session`, `/compress/summary`, `/cells/catalog` | various |
| `POST /api/v1/repl` | Chat (execute a turn) |

**Gap:** no `/api/v1/trading`. `heiwa_trading` is a separate app; the Trading
tier is a placeholder until a trading endpoint exists. Noted, not solved here.

## Architecture

### 1. Location (consolidation)

One canonical, cross-platform app at **`apps/heiwa_app/desktop/`**:

```
apps/heiwa_app/desktop/
  src-tauri/            # Tauri 2 Rust shell
    src/main.rs         # window + app lifecycle; registers commands
    src/proxy.rs        # api_get / api_post commands -> :7474; health
    tauri.conf.json     # window, dev/prod frontend URLs, minimal capabilities
    Cargo.toml
  src/                  # renderer client (choice locked before implementation)
    main.ts(x)
    app.ts(x)           # nav shell (two tiers) + header status canary
    lib/runtime.ts      # typed client over the proxy commands
    lib/types.ts        # types for consumed payloads (hand-written for S0)
    views/
      Chat.tsx Calendar.tsx Trading.tsx Agents.tsx
      Dashboard.tsx     # placeholder tier entry
  index.html, package.json, frontend config, tsconfig.json
```

Tauri 2 is cross-platform, so this retires the empty per-platform
`clients/{macos,windows}` dirs for desktop purposes (left in place for now;
removal is a later cleanup, not S0).

Renderer options before implementation:

1. **Tauri + existing Solid/Vite client** — fastest path because the cockpit
   already uses Solid/Vite + TypeScript. This is the recommended S0 if the goal
   is only to prove the native shell/runtime bridge now. It does not decide the
   background server or memory architecture.
2. **Tauri + vanilla TypeScript** — smallest dependency surface and keeps S0
   honest as a shell/canary, but gives up reusable cockpit components.
3. **Tauri + Rust/WASM renderer (Leptos/Yew/Sycamore)** — maximizes Rust sharing,
   but is heavier and riskier for S0. It should be chosen only if the desktop UI
   needs Rust-first rendering now, not just because the backend is Rust.

### 2. Runtime bridge — Rust proxy

`src-tauri` exposes two thin, typed Tauri commands:

- `api_get(path: String) -> Result<serde_json::Value, ApiError>`
- `api_post(path: String, body: serde_json::Value) -> Result<serde_json::Value, ApiError>`

Both proxy to `http://127.0.0.1:7474{path}` (reqwest), returning parsed JSON or
a typed `ApiError` (`Offline`, `Http(status)`, `Decode`). A small
`runtime_health()` wraps `GET /api/v1/runtime/snapshot`.

Rationale (high-quality choice): no CORS, runtime presence handled in one place,
and **future verticals add zero Rust** — they call new paths through the same
two commands. Runtime authority stays in the runtime; the app is pure display.

Port override: read `HEIWA_APP_PORT` env (fallback `7474`) so the proxy tracks a
non-default runtime port.

### 3. On-device state and queryable memory

S0 must keep a single write authority:

```
CLI or app input -> heiwa runtime API -> Rust services/read models
                 -> local state / SQLite stores / optional STDB reducers
```

The app must not mutate the local memory store, receipts DB, provider config, or
SpacetimeDB directly. The CLI and desktop app should call the same runtime
commands/endpoints so programmatic usage and in-app changes converge on one
state machine.

Current local storage truth:

- `~/.heiwa/state/` holds local runtime read models, worker heartbeats, dispatch
  requests/results, events, capabilities, approvals, and similar operational
  state.
- SQLite is already a valid local read/write store where it has a bounded
  runtime role: receipts use `~/.heiwa/receipts.db`, quota uses local SQLite,
  and sessions keep transcript JSON plus an FTS-backed `sessions.sqlite3` index.
- `/api/v1/memory` exists but currently returns an empty read model. A real
  Memory/Dashboard slice needs its own spec for local memory tables, FTS/vector
  indices, sensitivity labels, source refs, retention, and STDB mirror headers.

Target storage shape for future memory/context work:

- **Rust + SQLite locally:** fast local read models, FTS, embedding refs, receipt
  lookup, source refs, and redacted context selection live under `~/.heiwa/`.
- **Rust + SpacetimeDB online:** canonical reducer-governed sync/adjudication for
  sessions, leases, runs, artifacts, failures, policy, and evidence when
  configured.
- **Python workers:** allowed as isolated compatibility/R&D workers or promoted
  product modules, but they should read/write through Heiwa runtime APIs or a
  declared worker store. Python should not become the hidden desktop server or
  privileged memory owner by default.
- **WASM:** STDB publish/build flows can use compiled WASM modules, and reducers
  may execute in fresh WASM/JS module instances on the database side. Future
  local WASM plugin sandboxes can run deterministic modules inside the Rust
  runtime. Those are related portability tools, not the same thing as putting
  the desktop app's memory database in the WebView.

### 4. Information architecture (two tiers)

Nav shell with a persistent sidebar:

- **Consumer tier:** Chat · Calendar · Trading · Agents
- **Dashboard button** → advanced tier entry (Providers · Model Scorecard ·
  Memory · Personalization · Settings — listed, rendered as a single Dashboard
  placeholder in S0).

Each view is a placeholder that names its backing endpoint(s) as an on-screen
TODO, so the IA and wiring contract are visible without implementing verticals.

### 5. Data flow

```
view -> lib/runtime.ts (typed) -> tauri.invoke("api_get", {path})
     -> src-tauri proxy.rs -> http://127.0.0.1:7474/api/v1/* -> JSON
     -> typed -> render
```

### 6. Error handling

`ApiError::Offline` (runtime not reachable) → shell header renders an "offline"
state; views render an empty/offline placeholder rather than throwing. No retry
storms in S0 (single fetch on mount + manual refresh). Starting the runtime from
the app (`heiwa app start`) is a later enhancement, not S0.

## Canary (proof-of-wire, the core S0 deliverable)

The shell header renders **live runtime status** from
`GET /api/v1/runtime/snapshot` via the full path (view → runtime.ts → invoke →
proxy → :7474 → typed → render): reachable, version, uptime. Offline state when
the runtime is down. This is the "does the foundation work and wire cleanly"
test.

## Testing

- **Rust:** unit/integration test for `proxy.rs` — reachable path returns parsed
  JSON (against a local stub server), and offline path returns `ApiError::Offline`.
- **Frontend:** `tsc --noEmit` typecheck; a Vitest smoke for `runtime.ts`
  (mocked `invoke`) asserting it parses a snapshot payload and surfaces offline.
- **Recipe:** `just desktop-check` runs typecheck + `cargo test` for `src-tauri`.
- **Manual:** `just desktop-dev` (→ `tauri dev`) shows the window with live
  runtime status against a running `heiwa app start`.
- **Storage guard:** S0 tests assert the renderer only calls Tauri proxy/runtime
  helpers. It must not open SQLite, STDB, provider config, or local state files
  directly.

## Build / dev workflow

Add to `Justfile`:

- `desktop-dev` → `cd apps/heiwa_app/desktop && npm install && npm run tauri dev`
- `desktop-check` → frontend `tsc --noEmit` + `cargo test` in `src-tauri`

## Future verticals (forward context — out of S0)

Each becomes its own slice/spec, plugging into the established client and adding
runtime read models/endpoints where they do not already exist:
Chat (`/api/v1/repl` + WS), Calendar (`/life/*`), Agents (`/agents`,
`/approvals`, `/missions`), Dashboard contents (`/providers`, `/resource`,
`/routes`, `/rate-groups`, `/capabilities`, `/memory`), Trading (needs a new
endpoint). The skeleton's value is that these do not need a new desktop bridge.

## Open questions / risks

- **Tauri 2 toolchain** must be installed (`cargo install tauri-cli` / `@tauri-apps/cli`).
  Verify before implementation; document in the plan.
- **Renderer choice** must be confirmed before implementation. Default
  recommendation is Solid/Vite only if Devon accepts reusing the current cockpit
  stack for S0.
- **Snapshot payload shape** for the canary types must be confirmed against
  `runtime/snapshot` output during implementation (hand-write `types.ts` from the
  real payload).
- **Memory/Dashboard expectations** must stay scoped. `/api/v1/memory` is an
  empty read model today, so S0 can show only runtime status plus placeholders
  unless a separate memory read-model slice is approved.
- **Per-platform client dirs** (`clients/macos`, `clients/windows`) cleanup is
  deferred.

## References

- `HEIWA.md` (lines 15, 44, 46, 140, 579–581) — Heiwa.app + Tauri 2 canon
- `AGENTS.md` (lines 164, 179–180) — desktop gate + Tauri 2 rationale
- `apps/heiwa_shell/src/cmd/app.rs` — `:7474` server + `/api/v1/*` surface
- `apps/heiwa_app/clients/cockpit/src/lib/api.ts` — existing transport (reference)
- `docs/superpowers/plans/2026-06-02-macbook-private-server-mode-v0.md` — related
  private-server status surfacing
