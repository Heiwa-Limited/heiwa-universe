# Heiwa gateway: one authority, every surface

Status: proposed, 2026-11-24. Owner: Devon.

## Decision

The Heiwa server is the gateway. Desktop, web, mobile, Rex, Discord and outside agents all reach
Devon's machine, data and agents through it. We do not add another agent runtime or app. Side
systems that grew around Heiwa become feeders, then fold in:

| Today (outside Heiwa)                                                                  | Becomes                                                                                |
| -------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Hermes gateway (Discord bot, cron, its own agent loop on local gemma4; ~8 GB on disk)  | A thin Discord adapter that is an MCP client of Heiwa. Hermes retires.                 |
| `~/heiwa-root` collector (iMessage, notifications, app focus, AI usage into `root.db`) | A read-only `LifeSignals` service in `apps/server/src/native`, scoped like Apple apps. |
| `~/heiwa-ea` 06:00 brief and `~/heiwa-root/hygiene.py` (launchd)                       | Heiwa scheduled tasks with recorded outcomes.                                          |
| Rex Root/Heiwa sessions as the phone's view                                            | Kept for terminals; user-facing cards move to Heiwa clients.                           |

## What already exists

| Gateway need                         | Module                                                                                                                                       |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Approvals for real-world actions     | `native/NativeWorkflows.ts`: Mail, Calendar, Reminders and Notes proposals, review required by default ([Apple apps](../user/apple-apps.md)) |
| Mail without AppleScript cost        | `native/MailIndex.ts`: read-only Envelope Index                                                                                              |
| Push to the phone                    | `relay/AgentAwarenessRelay.ts` plus `infra/relay/src/agentActivity/ApnsClient.ts`                                                            |
| Scheduling, webhooks, delivery proof | `scheduledTasks/ScheduledTaskService.ts`, `RelayDeliveryProof.ts`                                                                            |
| Outside agents and bots              | `mcp/McpHttpServer.ts`, scoped sign-in ([Outside agents](../user/outside-agents.md))                                                         |
| Rich agent UI on every client        | `mcpApps/*`: MCP Apps render in the thread on web, desktop and mobile ([Visual replies](../user/html-renders.md))                            |
| Phone reachability                   | Tailscale HTTPS route and pairing (`heiwa pair --tailscale`, [Remote access](../user/remote-access.md))                                      |
| Identity                             | Clerk (desktop, web, Expo). No second auth provider.                                                                                         |

## Gaps and design

### 1. LifeSignals (read-only personal context)

A `LifeSignals` service reads the local life store and exposes it to agents through MCP tools:
`life_open_loops` (unanswered people threads, age, contact name), `life_day` (messages in/out,
notifications by app, focus time, AI usage) and `life_search` (metadata only).

- New MCP scope `life:read`, off by default, granted per client like Apple apps.
- Message bodies are never returned by default. A separate `life:read-content` scope adds them.
  Cloud-model threads see counts and names; bodies stay local unless that scope is granted.
- Collection still needs Full Disk Access. Keep the `root.db` collector (it runs inside Rex,
  which holds FDA) until Heiwa.app holds FDA itself, then move collection in-process.

Done when Claude Code or Codex in a terminal, signed in to Heiwa's MCP with `life:read`, gets
today's open loops, and a client without the scope gets a scope error. Unit tests cover the
redaction default.

### 2. Discord as a channel, not a second agent

A small adapter (Discord gateway connection plus Heiwa MCP client) maps:

- a DM or channel message to a Heiwa thread message (provider and model chosen by Heiwa routing);
- a pending `NativeWorkflows` proposal to a Discord message with Approve and Reject buttons, which
  call the same review endpoint the desktop uses;
- the daily brief to a post in `#executive-reports`.

It holds only the bot token and a Heiwa MCP credential with explicit scopes. It runs no model of
its own. Every model call goes through Heiwa's providers and the safeagents guard.

Done when a DM starts a Heiwa thread, an Approve button applies a real Reminders proposal once
(idempotent on retry), and the brief posts on schedule. Then quarantine the Hermes LaunchAgent and
reclaim its disk.

### 3. A2UI surfaces

[A2UI 1.0](https://developers.googleblog.com/a2ui-10-build-delightful-fast-reliable-agentic-experiences/)
is a declarative, Apache-2.0 format for agent-generated UI. Heiwa ships it inside an MCP App: one
app resource bundles the A2UI web renderer, so every client that renders MCP Apps (web, desktop,
mobile, and Claude) shows the same surface without a per-platform renderer. Native renderers
(SwiftUI, Compose) are a later optimization, not a prerequisite.

First surfaces, each backed by an existing service:

1. **Proposal card**: what changes, source evidence, Approve/Reject (NativeWorkflows).
2. **Daily brief**: schedule, open loops, markets, hygiene, each section dated with its source.
3. **Hygiene report**: updates, vulnerabilities, exposure, disk, with one-tap follow-ups.

The Discord adapter renders the same A2UI documents to embeds and buttons (a strict subset). Unsupported
components fall back to a link to the thread.

Done when the proposal card renders identically on desktop and mobile, an approval from either is
recorded once, and the card's A2UI document passes schema validation in CI.

### 4. Schedules inside Heiwa

Move the 06:00 brief and the weekly hygiene run into `ScheduledTaskService` so runs, failures and
outcomes are recorded and visible on every client. launchd keeps only what Heiwa cannot do yet
(FDA collection).

### 5. Install-as-app companions (optional)

Claude's "Install as app" (beta, Mac and Windows, online and signed in to Claude, counts against
plan usage) can turn an artifact into a menu-bar companion. Useful for a glanceable Mac view of the
brief; not part of the gateway, and not available on iPhone.

## Safety

- Model traffic from bots and local agents passes through the safeagents guard
  (`local.heiwa.safeagents`, log-only during the trial, enforce after a week of clean journals).
- Real-world actions stay proposals with review required.
- Agents cannot hard-delete on this machine (Claude Code deny rules and hook, Codex `forbidden`
  rules, Hermes `approvals.deny`); deletes go to the Trash or a dated quarantine with a log.
- Life data is local-first: bodies off by default, local models preferred for summarization.

## Phases

| Phase               | Scope                                                                                                                           | Exit check                                                         |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| 0 (done 2026-11-24) | Root collector, EA brief fixes, hygiene job, safe deletes, safeagents proxy, Tailscale HTTPS route, dependency floors (PR #194) | All running; tests green                                           |
| 1                   | LifeSignals service and MCP tools with scopes                                                                                   | Scope and redaction tests; terminal agent reads open loops         |
| 2                   | Discord adapter over MCP; retire Hermes                                                                                         | DM to thread, button approval, scheduled brief; Hermes quarantined |
| 3                   | A2UI MCP App with proposal, brief and hygiene cards                                                                             | Same card on desktop and mobile; single recorded approval          |
| 4                   | Brief and hygiene as Heiwa scheduled tasks                                                                                      | Runs visible on every client; launchd agents retired               |

Measure each phase the way the operations guide asks: review minutes, corrections, and incorrect
accepted actions over three real days, before starting the next.

## Non-goals

A new native app, a second identity provider (WorkOS), GPS spoofing of the phone (iOS does not
allow it; the Tailscale exit node changes only the network location).
