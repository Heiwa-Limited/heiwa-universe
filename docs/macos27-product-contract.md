# Heiwa for macOS 27

Product direction: 2026-09-11. Classification: Intake, Execution, Evidence.
This records the user's current direction and the implementation needed to meet
it. The delivery table distinguishes current changes from unfinished work.

## The application

Heiwa is the user's everyday chat and work home. A person downloads an app,
opens it, establishes a local workspace, connects their own resources, and starts
working. They do not need this checkout, a terminal, developer tools, or the
maintainer's accounts. The current supported product target is macOS 27; iOS is
a later client over shared contracts, with platform-specific capabilities.

Use the existing TypeScript/Solid desktop in `apps/heiwa_app/desktop` and its
Tauri native host as the production integration point. Use Swift for Apple
framework adapters and native controls where macOS behavior requires them.
Rust owns durable state, routing, execution, approval/effect policy, provider
supervision, cancellation, recovery, and evidence. Avoid a second state authority
in Swift, TypeScript, or an external prototype. Earlier standalone design studies
are references; they are not distributable product evidence.

The home shows recent and active sessions, unfinished work, upcoming commitments,
and useful artifacts. A session may belong to a project or stand alone. A project
groups context, resources, instructions, and sessions; it does not own their
execution lifetime. Session, Work, turn, run, and artifact identities must remain
stable when a session moves between projects or windows.

The permanent composer exists in every Heiwa window: Home, Calendar, Mail,
Reminders, Media, project, and session. Submitting opens an anchored compact
response panel while the current surface stays visible. Expanding opens the
same session. Closing the panel does not cancel work. Cancel, retry, continue,
inspect, and edit must target explicit turn/run identities. Context menus and
object inspectors hold detailed actions; the main window should not repeat a
wall of shallow buttons.

## Onboarding that works for a stranger

1. Launch a bundled Rust runtime, or adopt a compatible runtime belonging to the
   same user/profile. Prove protocol, identity, authentication, and data-root
   compatibility; a listening port alone is insufficient.
2. Resolve all per-user paths through `HeiwaPaths`. Create a local installation
   identity and empty workspace. Never seed another person's sessions, projects,
   calendar events, files, or model assumptions.
3. Passively detect installed applications, supported CLIs, and existing Heiwa
   account references. Show what was observed, its scope, and what remains
   unknown. Do not launch apps, read their content, or run inference to draw an
   onboarding card.
4. Let the user enter with no provider. Provider readiness, workspace setup, and
   connector authorization are separate states. The resources panel remains
   available later, including after permission denial or an expired sign-in.
5. Connect resources through supported account flows, Apple permission prompts,
   and system pickers. Select actual accounts/calendars/lists/folders and import
   bounded metadata; then fetch content when a turn needs it. Reflect added,
   renamed, removed, revoked, and unavailable resources without duplicating them.
6. Save choices atomically under the current installation. Reopening the app
   restores work and selections. Disconnecting must stop future access and
   explain the treatment of already imported content.

The current passive scanner checks standard system and user Applications folders
and the existing provider command resolver. It is not an exhaustive Launch
Services inventory: renamed apps, unusual install locations, and remote resources
may be absent. Unknown means unknown, not disconnected or inaccessible.

## Inference and creation across providers

One provider can expose several execution channels. Keep provider, account,
channel, exact model, effort controls, tools, modality, billing source, and
execution evidence separate. A ChatGPT or Claude app installed on disk does not
grant Heiwa an API key, subscription allowance, or image tool.

| Resource family | Channels to support | Admission evidence |
| --- | --- | --- |
| OpenAI / ChatGPT | Supported Codex/account execution, direct API, supported app/tool bridges | Exact account/channel sign-in, supported model/tool inventory, scoped execution result |
| Anthropic / Claude | Supported Claude Code/account execution, API, app tools/connectors | Model versus tool-produced artifact capabilities, actual tool result |
| Google / Antigravity | Antigravity CLI/tools, Gemini CLI, Google APIs | Account/region/tool availability and verifiable artifact output |
| Local inference | Ollama, MLX, llama.cpp and Apple Foundation Models adapters | Endpoint/device availability, installed model, supported modalities, measured execution |
| Additional providers and tools | Registered adapters and MCP integrations | Versioned capability contracts, authorization, health and execution evidence |

Media supports image, video, audio/music, and derived formats through whichever
eligible channel can complete the task. Image generation is not hardcoded to one
vendor. Distinguish native image models, image tools run by an agent, code-rendered
charts/SVGs, and edits of an existing image. Preserve the actual producing tool
and provider in the artifact receipt, including when another model planned it.

Provider facts checked on 2026-09-11:

- OpenAI documents [ChatGPT image creation and editing](https://help.openai.com/en/articles/11084440-im).
- Anthropic distinguishes [Claude's diagrams and visual/code output from native photo/illustration generation](https://support.claude.com/en/articles/9002504-can-claude-produce-images).
- Antigravity documents a [generate_image tool](https://www.antigravity.google/docs/hooks)
  and [headless CLI execution](https://antigravity.google/docs/cli/headless/).
  Its soft permission denials mean an exit status alone is insufficient proof.

These describe provider capabilities, not capabilities already wired through
Heiwa. Supported account execution and direct API billing remain distinct.
Heiwa must never extract app/browser session secrets to manufacture an API.

## Each turn is a routing and execution decision

The original brief makes Astra the preferred architecture/control model for
complex decomposition and final integration where the user's account admits it.
It is not the mandatory worker for every action. Model identifiers and account
availability come from current provider facts, not names copied from this document.

The Rust pipeline is:

`intent + bounded context → required outcome/capabilities → eligible channels → model and effort decision → bounded task graph → tools/workers → verified result`

Deterministic actions should not require model inference. For model work, choose
model and effort separately using capability, user constraints, observed quality,
latency, account allowance, metered cost, privacy, and device pressure. Routine
work need not call an architect first. Higher effort and delegation must justify
their cost. Unknown prices or allowances remain unknown; do not report them as free.

Delegation needs explicit task ownership, input/output contracts, prerequisites,
budgets, deadlines, cancellation, and acceptance checks. Parallel workers may not
silently overwrite the same files or duplicate external effects. Reuse a worker
where it helps; escalate on failed acceptance evidence, not self-reported
confidence alone. Prefer independent review for consequential outputs when an
eligible reviewer exists. Save measured results to improve future decisions.

No product can promise perfect delegation. Heiwa must make its decisions
inspectable and recoverable, and improve cost per accepted result with evidence.

The execution inspector normalizes provider events into the existing Rust/operator
event contracts: assignment, model start, tool request/result, file/command/browser
activity, artifact creation, verification, usage, failure, cancellation, and
completion. The user should inspect a running worker, its source references, and
its outputs inside Heiwa. Show plans, status summaries, provider-supplied summaries,
diffs, and receipts. Never invent or expose hidden chain-of-thought. Keep usage
and subscription/API billing source attached to the actual execution channel.

## Context that follows the user

Each Heiwa window publishes typed view state: window, surface, session/project,
selected object, visible range, resource revisions, and bounded recent navigation.
Context uses source IDs and excerpts; the Rust runtime admits only the material
needed for the turn. The user can inspect and remove included context before it
leaves the machine. Sensitivity, freshness, retention, and provenance travel with
each item. Source content is data, never an instruction granting tool authority.

Within Heiwa, use structured app state. Outside Heiwa, use a supported connector,
share/picker flow, browser integration, or explicitly enabled accessibility/screen
capture. macOS does not grant arbitrary access to every app. App Intents expose
an app's declared actions; they are not universal access to every application's
private history. Apple documents [App Intents and View Annotations](https://developer.apple.com/apple-intelligence/).

## Apple integration depth

| Surface | User-visible depth required | Integration boundary |
| --- | --- | --- |
| Calendar | Account/calendar selection, recurrence, timezone, conflicts, event details, updates, cancellation | Swift EventKit bridge; stable source IDs, permission recovery and write receipts |
| Reminders | Lists, due times, recurrence, completion, project links, event/message-to-reminder | EventKit bridge; actual list ownership and persistence verification |
| Mail | Account/mailbox browsing, threads, selected body/attachments, editable drafts, send status | Supported macOS Mail automation/extension capabilities or authenticated mail-provider adapters; MailKit alone is not a general mailbox API |
| Files / iCloud Drive | Native picker, project roots, bookmarks, search, previews, revisions, export | Security-scoped document access; Rust attachment/artifact lifecycle |
| Notes / Contacts / Photos | Scoped selections, source links, authorized reads, useful supported writes | Framework or documented app-specific adapters; expose unsupported operations honestly |
| Safari / Messages / Shortcuts | Selected context, supported actions, handoff and execution receipts | Per-app capability and permission boundaries; no private-database shortcut |
| Pages / Numbers / Keynote / Music | Selected documents or media, supported editing/playback, export | Specific adapters; playback/library access is separate from generation |

macOS 27 is the platform baseline. Follow the actual SDK and [Apple release notes](https://developer.apple.com/documentation/macos-release-notes/macos-27-release-notes).
Foundation Models can be an Apple provider bridge, while Rust retains routing
authority. Private Cloud Compute eligibility/entitlements must be proven for the
chosen distribution route. Do not base media generation on removed ImageCreator
APIs; [Apple's transition notice](https://developer.apple.com/news/?id=dz9wvq0r)
describes the change. Native materials, keyboard navigation, reduced motion,
accessibility, window restoration, and system permissions are acceptance work.

## Delivery and acceptance

| Work | Current change / gap | Proof required |
| --- | --- | --- |
| New-user workspace | Implemented local welcome completion independently of provider readiness; installation-bound atomic record | Fresh temporary profile, restart, corrupt/future record, failed write, profile isolation |
| Resource onboarding | Implemented passive catalog and reopenable panel; Calendar action uses current connector surface | App/CLI/account distinctions, error/retry, no fabricated resources; actual import remains connector-specific |
| Permanent chatbar | Implemented response panel over active surface, shared conversation renderer, retained submission drafts | Stream while Calendar remains open, close/reopen, expand without resubmission, submission failure |
| Packaging | macOS minimum set to 27; build refuses missing/unusable bundled runtime; SDK 27 selects Apple clang instead of incompatible bundled LLD | Missing runtime and SDK 27 linker regressions; final signed app still needs clean-machine installation certification |
| Sessions and projects | Implemented Rust journal catalog, named standalone/project sessions, rename/move/archive metadata, sidebar and guarded session switching | Service restart and authenticated API tests; frontend late-frame and draft isolation regressions; structured turn context remains unfinished |
| Home | Dark chat/work home with real catalog rows and conditional today information; machine diagnostics inside resource setup | Empty/error/loaded states use actual runtime data; no fabricated sessions or commitments |
| Native lifecycle | Window-scoped stream cancellation, explicit runtime port, fresh-profile private local authentication | Observation cancellation does not cancel work; concurrent first launch preserves one credential; malformed existing credentials remain unchanged |
| Attachments and Media | Unfinished in repository desktop | File/image picker, paste/drop, size/type checks, durable attachment references, provider transport, preview/export, cancel/restart, actual media output receipts |
| Provider setup/routing | Existing adapters and fleet health; onboarding offers detection and explicit verification | In-app account flows, exact channel/model/effort/modality admission, account-specific image execution, budgets, measured feedback |
| Apple productivity | Existing Calendar connector; explicit bounded native Mail metadata read and snapshot display; broader actions unfinished | Fresh-user permission prompts, selected-resource import, round-trip update/completion/send proof, revoked access |
| Public installation | Existing bundle/runtime/release plumbing; this change is local and unpublished | Developer ID signing, hardened runtime/entitlements, notarization, stapling, Gatekeeper, first launch without checkout/PATH, signed update/recovery |

Next implementation follows user value across the stack: durable standalone and
project sessions with per-window context; attachments and a provider-backed Media
flow; then Apple event/message-to-reminder and draft workflows over shared Rust
effect services. Each vertical slice must work from a fresh profile through a
real artifact or persisted source-app result. Do not replace these outcomes with
additional dashboard tiles or capability declarations that have no executor.
