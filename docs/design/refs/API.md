# Heiwa Runtime API

Updated: 2026-04-22
Status: working HTTP + WebSocket contract for the local `heiwa` runtime and cockpit

If this conflicts with [`HEIWA.md`](/Users/dmcgregsauce/heiwa-universe/HEIWA.md), [`BRAND.md`](./BRAND.md), or [`CLI.md`](./CLI.md), those win.

## Purpose

Define the contract between:

- the installed local runtime (`heiwa`, `heiwa app`, `heiwa_core`)
- the localhost cockpit SPA
- the remaining legacy operator HTML pages during migration

This is a local runtime contract, not a public `heiwa.ltd` API.

## Scope

IN:

- localhost HTTP resources for cockpit reads and mutations
- localhost WebSocket streams for events and REPL
- auth expectations for local operator access
- compatibility mapping from old endpoints to the new `/api` and `/ws` layout

OUT:

- public marketing/status/docs endpoints on `heiwa.ltd`
- provider inference internals
- hosted multi-tenant auth or account systems
- exact Rust module boundaries

## Runtime Boundary

- default local bind: `127.0.0.1:8787`
- cockpit dev server: `127.0.0.1:5173`
- Vite proxies `/api/*` and `/ws/*` to the local runtime
- operator data, sessions, memory, and secrets stay under the local runtime and `~/.heiwa/`

Public surfaces on `heiwa.ltd` are out of scope except for the read-only public status shell.

## Versioning

Use a versioned local HTTP namespace:

- `GET /api/v1/...`
- `POST /api/v1/...`
- `WS /ws/v1/...`

During migration, existing unversioned endpoints may remain as compatibility shims:

- `/missions`
- `/approvals`
- `/history`
- `/rate-groups`
- `/auth/me`
- `/auth/providers`
- `/call/heiwa_get_cells_catalog`
- `/ws/operator`

The cockpit should prefer the versioned contract. Legacy static pages may keep using the old paths until they are retired.

## Auth Model

Default model:

- localhost-only runtime
- bearer token for sensitive reads and all mutations
- token source: local runtime login/bootstrap, stored under `~/.heiwa/`

Client behavior:

- send `Authorization: Bearer <token>` on authenticated routes
- treat `401` and `403` as operator re-auth / token-refresh states
- do not assume cookies

Anonymous routes are allowed only for clearly public-safe local reads such as:

- provider catalog JSON already bundled in the client
- local static assets
- optionally a basic unauthenticated health probe

## Envelope Rules

HTTP success shape:

```json
{
  "ok": true,
  "data": {}
}
```

HTTP error shape:

```json
{
  "ok": false,
  "error": {
    "code": "approval_required",
    "message": "Approval required before execution.",
    "details": {}
  }
}
```

Error code guidance:

- `unauthorized`
- `forbidden`
- `not_found`
- `usage_error`
- `validation_error`
- `approval_required`
- `provider_auth_failed`
- `offline_only`
- `conflict`
- `internal_error`

## Core Resources

### Session

`GET /api/v1/session`

Returns the local operator session and runtime summary.

Response:

```json
{
  "ok": true,
  "data": {
    "operator_id": "local-devon",
    "hostname": "MacBook-Pro",
    "runtime_version": "0.1.0",
    "channel": "stable",
    "default_route_role": "code",
    "app_url": "http://127.0.0.1:8787"
  }
}
```

Compatibility source:

- old `GET /auth/me`

### Providers

`GET /api/v1/providers`

Returns live provider connection state, not the marketing catalog.

Response fields:

- `provider_id`
- `display_name`
- `auth_kind`
- `status`
- `rate_group`
- `default_model`
- `last_validated_at`
- `last_error`
- `supported_lanes`

Compatibility source:

- old `GET /auth/providers`

Mutation endpoints:

- `POST /api/v1/providers/{provider_id}/link`
- `POST /api/v1/providers/{provider_id}/unlink`
- `POST /api/v1/providers/{provider_id}/test`

### Hooks

`GET /api/v1/hooks`

Returns local provider hook posture from live home config. This is read-only:
Heiwa observes provider-owned hook surfaces and reports drift, command presence,
audit paths, and unsupported parity instead of pretending every provider exposes
the same hook API.

Response fields:

- `summary.source`
- `summary.active`
- `summary.degraded`
- `summary.unconfigured`
- `summary.unsupported`
- `summary.delegated`
- `providers[].provider_id`
- `providers[].status`
- `providers[].config_path`
- `providers[].generated_config_status`
- `providers[].audit_file`
- `providers[].events[].event`
- `providers[].events[].matcher`
- `providers[].events[].hooks[].command_path`

### Routes

`GET /api/v1/routes`

Returns current route policy and effective provider/model selection by role.

Response fields:

- `role`
- `provider`
- `model`
- `source`
- `fallbacks`
- `offline_capable`

Example:

```json
{
  "ok": true,
  "data": {
    "routes": [
      {
        "role": "code",
        "provider": "ollama",
        "model": "qwen3.5:9b",
        "source": "default",
        "fallbacks": ["gemini-cli", "claude-code"],
        "offline_capable": true
      }
    ]
  }
}
```

Mutations:

- `POST /api/v1/routes/{role}`
- `POST /api/v1/routes/test`

### Missions

`GET /api/v1/missions`

Query params:

- `status`
- `limit`
- `cursor`

Response fields:

- `mission_id`
- `prompt`
- `status`
- `intent_class`
- `target_tool`
- `target_model`
- `summary`
- `updated_at`

Compatibility source:

- old `GET /missions`
- old `GET /missions?status=running&limit=50`

Mutations:

- `POST /api/v1/missions/{mission_id}/pause`
- `POST /api/v1/missions/{mission_id}/resume`
- `POST /api/v1/missions/{mission_id}/cancel`

### Approvals

`GET /api/v1/approvals`

Returns pending approval queue.

Fields:

- `approval_id`
- `mission_id`
- `risk_level`
- `summary`
- `requested_at`
- `expires_at`
- `requested_by`

Compatibility source:

- old `GET /approvals`

Mutations:

- `POST /api/v1/approvals/{approval_id}/grant`
- `POST /api/v1/approvals/{approval_id}/deny`

### Rate Groups

`GET /api/v1/rate-groups`

Returns live group health and priority, not just the static catalog.

Fields:

- `group_id`
- `priority`
- `status`
- `providers`
- `quota_state`
- `notes`

Compatibility source:

- old `GET /rate-groups`

### History

`GET /api/v1/history`

Returns recent session/run summary for the operator.

Fields:

- `sessions`
- `recent_runs`
- `artifacts`
- `cursor`

Compatibility source:

- old `GET /history`

### Trace

`GET /api/v1/traces`
`GET /api/v1/traces/{trace_id}`

Used by future `trace` cockpit surfaces and `heiwa trace ...`.

Fields:

- `trace_id`
- `session_id`
- `mission_id`
- `route`
- `receipts`
- `artifacts`
- `started_at`
- `ended_at`

### Memory

`GET /api/v1/memory`
`GET /api/v1/memory/{entry_id}`

Used by future `memory` cockpit surfaces and `heiwa memory ...`.

Fields:

- `entry_id`
- `scope` (`user`, `project`, `session`)
- `title`
- `summary`
- `source`
- `updated_at`

Mutations:

- `POST /api/v1/memory/ingest`
- `DELETE /api/v1/memory/{entry_id}`

### Agents

`GET /api/v1/agents`
`GET /api/v1/agents/{agent_id}`

Used by future `agent` cockpit surfaces and `heiwa agent ...`.

Fields:

- `agent_id`
- `parent_id`
- `status`
- `role`
- `started_at`
- `last_event_at`

Mutations:

- `POST /api/v1/agents/spawn`
- `POST /api/v1/agents/{agent_id}/kill`
- `POST /api/v1/agents/{agent_id}/attach`

### Cron

`GET /api/v1/crons`
`GET /api/v1/crons/{job_id}`

Used by future `cron` cockpit surfaces and `heiwa cron ...`.

Fields:

- `job_id`
- `name`
- `schedule`
- `status`
- `last_run_at`
- `next_run_at`

Mutations:

- `POST /api/v1/crons`
- `POST /api/v1/crons/{job_id}/run`
- `DELETE /api/v1/crons/{job_id}`

### Cells Catalog

`GET /api/v1/cells/catalog`

Read-only catalog for the existing cells page.

Compatibility source:

- old `POST /call/heiwa_get_cells_catalog`

Migration rule:

- keep the old POST reducer path only as a shim
- cockpit and future pages should use the GET resource

## WebSocket Channels

### Event Stream

`WS /ws/v1/events`

Unified operator event stream for:

- mission updates
- approval queue changes
- agent lifecycle changes
- cron state changes
- route changes
- provider state changes

Envelope:

```json
{
  "type": "mission.updated",
  "ts": "2026-04-22T18:45:00Z",
  "data": {}
}
```

Event types:

- `mission.created`
- `mission.updated`
- `approval.created`
- `approval.resolved`
- `agent.spawned`
- `agent.exited`
- `cron.updated`
- `provider.updated`
- `route.updated`
- `trace.completed`

Compatibility source:

- old `WS /ws/operator`

### REPL Stream

`WS /ws/v1/repl`

Bi-directional stream for the browser REPL surface.

Client → server:

```json
{
  "type": "input",
  "session_id": "sess_123",
  "text": "summarize recent routing changes"
}
```

Server → client:

```json
{
  "type": "output",
  "session_id": "sess_123",
  "stream": "stdout",
  "chunk": "Routing now prefers ollama for code.\n"
}
```

Other server event types:

- `footer`
- `route`
- `approval_required`
- `error`
- `done`

## Public Status Contract

The public status shell is not the cockpit, but it still needs a stable narrow stream.

Keep separate from cockpit auth and operator state:

- `WS /status/ws`
- `GET /status/health`

Rules:

- read-only
- no operator secrets
- no approval or mission detail
- safe to expose publicly through `api.heiwa.ltd`

## Compatibility Map

| Legacy path                          | New contract                |
| ------------------------------------ | --------------------------- |
| `GET /auth/me`                       | `GET /api/v1/session`       |
| `GET /auth/providers`                | `GET /api/v1/providers`     |
| none                                 | `GET /api/v1/hooks`         |
| `GET /missions`                      | `GET /api/v1/missions`      |
| `GET /approvals`                     | `GET /api/v1/approvals`     |
| `GET /rate-groups`                   | `GET /api/v1/rate-groups`   |
| `GET /history`                       | `GET /api/v1/history`       |
| `POST /call/heiwa_get_cells_catalog` | `GET /api/v1/cells/catalog` |
| `WS /ws/operator`                    | `WS /ws/v1/events`          |
| placeholder `/api/routes`            | `GET /api/v1/routes`        |
| placeholder `/ws/repl`               | `WS /ws/v1/repl`            |

## Cockpit Route Mapping

| Cockpit route       | HTTP / WS dependency                                                           |
| ------------------- | ------------------------------------------------------------------------------ |
| `/`                 | `GET /api/v1/session`, `GET /api/v1/providers`                                 |
| `/providers`        | `GET /api/v1/providers` plus bundled `providers.json` for descriptive metadata |
| `/routes`           | `GET /api/v1/routes`, `WS /ws/v1/events`                                       |
| `/hooks`            | `GET /api/v1/hooks`                                                            |
| `/repl`             | `WS /ws/v1/repl`                                                               |
| future `/missions`  | `GET /api/v1/missions`, `WS /ws/v1/events`                                     |
| future `/approvals` | `GET /api/v1/approvals`, `WS /ws/v1/events`                                    |
| future `/history`   | `GET /api/v1/history`                                                          |
| future `/memory`    | `GET /api/v1/memory`                                                           |
| future `/trace`     | `GET /api/v1/traces`                                                           |
| future `/agents`    | `GET /api/v1/agents`, `WS /ws/v1/events`                                       |
| future `/cron`      | `GET /api/v1/crons`, `WS /ws/v1/events`                                        |

## Implementation Notes

- prefer one runtime-owned HTTP server instead of adding a Node server tier
- prefer one runtime-owned event stream over multiple narrowly scoped WebSocket channels except for REPL
- keep JSON shapes stable enough for `--json` CLI output to mirror them where useful
- the cockpit should consume the same nouns chosen in `CLI.md`: `Provider`, `Route`, `Mission`, `Approval`, `Trace`, `Memory`, `Agent`, `Cron`

## What This Spec Does Not Decide

- exact Rust handler file layout
- whether the runtime internally uses STDB subscriptions, in-memory state, or both to fulfill each route
- whether UDS is added later beneath the localhost HTTP facade
- exact persistence schema for memory, trace, cron, or agent objects
