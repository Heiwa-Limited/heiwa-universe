# What Heiwa takes from the foundation

Heiwa is a fork of T3 Code. This page records, per subsystem, whether Heiwa builds on it, keeps it
as inherited, ships it switched off, or intends to remove it, and why. Read it before adding a
Heiwa feature: most of what a personal-work agent needs (a scheduler, webhooks, approvals,
notifications, usage accounting, a remote stack) already exists here, and a second copy of any of
it is a defect.

Verdicts:

- **Build on**: Heiwa features sit on this. Extend it; do not build a parallel one.
- **Keep**: works as inherited and stays. Not exercised for Heiwa beyond CI unless noted.
- **Off**: compiled in but inert in Heiwa builds, because no endpoint or key is configured.
- **Replace** / **Remove**: scheduled to go. Do not add callers.

## Server (`apps/server/src`)

| Subsystem                                                                                                   | Verdict  | What Heiwa uses it for, and what is missing                                                                                                                                                                                                                                                         |
| ----------------------------------------------------------------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `orchestration-v2`                                                                                          | Build on | The event log, projections, command receipts and outbox commit together, and the effect worker runs side effects after the commit. Any Heiwa side effect that must survive a crash belongs here. Apple proposals do not use it yet: they are rows with a status column in `heiwa_native_proposals`. |
| `native`, `mcp/toolkits/native`, `native/apple`                                                             | Build on | Heiwa's own. Read, propose, approve, apply, read back and record for Calendar, Reminders, Notes and Mail. Mail saves drafts and never sends. New Apple apps join this pipeline as another `NativeApp`, not as a new service.                                                                        |
| `scheduling`, `scheduledTasks`                                                                              | Build on | Interval, fixed-time and webhook triggers (HMAC check, replay and coalesce keys, filters) that start an agent thread. This is Heiwa's unattended intake, bound by the first rule below.                                                                                                             |
| Thread approvals and `ThreadNotificationCoordinator` (web)                                                  | Build on | Threads that need approval or input raise a sound, a toast, a desktop notification and a dock badge. Apple proposals stream to the client (`native.subscribe`) and use the same path. The Approvals page lists both. Not on mobile yet.                                                             |
| `textGeneration`                                                                                            | Build on | One structured model call without a thread (titles, commit messages). The cheap path for extraction and classification. Drivers exist for Claude, Codex and Antigravity; the local provider has none.                                                                                               |
| `provider/local`                                                                                            | Build on | Ollama over ACP as an ordinary provider. The default for bulk or unattended work, where a cloud call per email is not justified.                                                                                                                                                                    |
| `provider`, `packages/provider-*`                                                                           | Keep     | Provider adapters own their protocols, auth and quotas. Heiwa adds no logic to them.                                                                                                                                                                                                                |
| `usage`                                                                                                     | Build on | Per-provider usage, limits and pricing from local transcripts. The input for routing by cost.                                                                                                                                                                                                       |
| `htmlRender`                                                                                                | Build on | Publishes an HTML page into a thread with a rendered preview. The output format for reports (daily brief, finance, market).                                                                                                                                                                         |
| `background`                                                                                                | Build on | Decides whether background work should run from client activity and host power. Unattended intake must ask it before running.                                                                                                                                                                       |
| `persistence`, `auth`, `secrets`                                                                            | Build on | SQLite and migrations, pairing and scopes, the secret store. New Heiwa tables are migrations here; new protected RPCs get a scope.                                                                                                                                                                  |
| `mcp` (other toolkits), `mcpApps`                                                                           | Build on | Every capability is a service method reached by RPC, MCP tool and CLI.                                                                                                                                                                                                                              |
| `terminal`, `workspace`, `project`, `vcs`, `git`, `checkpointing`, `sourceControl`, `pullRequest`, `review` | Keep     | The coding workspace. Code projects remain a first-class use.                                                                                                                                                                                                                                       |
| `preview`                                                                                                   | Keep     | The in-app browser and its automation broker. The route for web tasks that have no API.                                                                                                                                                                                                             |
| `device`                                                                                                    | Keep     | Simulator and device control for agents. Not exercised for Heiwa.                                                                                                                                                                                                                                   |
| `peer`                                                                                                      | Keep     | Links between environments and remote delegation, behind a beta flag. The route to a second Mac or an always-on server.                                                                                                                                                                             |
| `guest`                                                                                                     | Keep     | Read-only thread sharing. Not exercised for Heiwa.                                                                                                                                                                                                                                                  |
| `cloud`, `relay`                                                                                            | Off      | T3 Connect: account sign-in, managed tunnels and mobile push through upstream's relay. Heiwa builds set no relay URL or key. Remote access is the local network or Tailscale. Mobile push depends on a relay, so it is unavailable until Heiwa runs its own.                                        |
| `telemetry`                                                                                                 | Off      | No endpoint is configured and there is no opt-in. `observability`, `resourceTelemetry` and `diagnostics` are local and stay.                                                                                                                                                                        |
| `desktopUpdate`, desktop `updates`                                                                          | Off      | No update feed. `vp run desktop:channel` installs builds.                                                                                                                                                                                                                                           |
| `triage`                                                                                                    | Remove   | Clones upstream's repository to triage upstream's bugs.                                                                                                                                                                                                                                             |

## Clients, infrastructure and packages

| Part                                                                 | Verdict  | Notes                                                                                                                                      |
| -------------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `apps/desktop`, `apps/web`                                           | Build on | The product. Desktop wraps web.                                                                                                            |
| `apps/mobile`                                                        | Keep     | Not built or run for Heiwa. Bundle ids and URL schemes are still upstream's. It can pair over the local network or Tailscale once renamed. |
| `apps/marketing`                                                     | Replace  | Upstream's site.                                                                                                                           |
| `infra/relay`                                                        | Off      | Not deployed. The candidate for a self-hosted relay if phone push is wanted.                                                               |
| `infra/task-cache`                                                   | Remove   | Upstream's CI cache service. Heiwa's CI does not use it.                                                                                   |
| `packages/source-control-*`, `secret-provider-*`, `ssh`, `tailscale` | Keep     |                                                                                                                                            |
| `native/resource-monitor`, `browser-secret`, `libghostty-vt`         | Keep     |                                                                                                                                            |
| `native/hyprland-snap-shot`, `native/kde-snap-shot`                  | Keep     | Linux only.                                                                                                                                |
| `evals`                                                              | Build on | Model-facing text for Heiwa tools is checked here (`evals/heiwa-native`).                                                                  |
| `.repos`                                                             | Keep     | Read-only references agents rely on for Effect patterns.                                                                                   |

## Rules that follow

1. **Unattended turns do not write.** A turn that the user did not type at a client (a schedule,
   a webhook, another agent, a usage-limit resume, a subagent) reads untrusted content such as
   mail, web pages and webhook bodies with nobody watching. It may propose Apple changes for
   review; `heiwa_native_execute` refuses it whatever the app's write mode. The server stamps
   some of its own messages `createdBy: "user"`, so "attended" is never `createdBy` alone.
2. **One inbox.** Anything waiting on the user (an Apple proposal, a thread approval, an uncertain
   write) is listed in one place and raises the same notification. A new kind of pending item
   joins that list.
3. **Heiwa never sends, pays or trades.** It drafts and proposes. Sending mail or messages and
   moving money stay in the native app.
4. **Local first for bulk.** Extraction and classification over many items use the local provider
   through `textGeneration`. A cloud provider is for work the user started.
5. **Nothing depends on upstream's services.** A feature that needs the relay, upstream sign-in or
   upstream's update feed is unavailable in Heiwa, and the UI should not offer it.
