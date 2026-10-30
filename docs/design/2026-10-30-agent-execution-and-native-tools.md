# Governed native agent execution

Status: implemented locally; installed-device acceptance is recorded separately.
Classification: Execution, Intake, Evidence.

Heiwa's desktop turns previously had no host-tool scope. The operator executed
one JSON tool batch and then treated a single follow-up as the final answer.
Codex flattened role messages into `codex exec`, while Ollama used terminal
output. This made a visible provider connection a poor predictor of useful work.

## Decision

Retain the existing Rust runtime, durable operator journal, DREX gates, Solid
client, Tauri wrapper, and Swift Apple helper. Refactor their execution seam
rather than replace the application with another independent control plane.

A provider can request a typed host tool and await its result. The admitted
adapter must preserve this channel. Heiwa records the tool intent, applies its
existing approval policy, executes the scoped service, records its receipt, and
returns the result. Native function calls are never reconstructed from final
answer text. Legacy text-only API channels keep the bounded JSON protocol.

The common operator supports twelve model rounds and forty-eight tools. Native
Codex and Ollama sessions have their own bounded loops within that tool budget.
Cancellation stops the turn, records uncertain in-flight tool outcomes, and
stops the provider's entire process group, including launcher descendants.
A provider failure after tool dispatch must not trigger automatic replay.
Usage and cost truth accumulate across stages. Large tool results become local
artifacts; only their bounded preview is returned to the model.

Ordinary owner chat receives a private per-conversation workspace. The Files
surface can select a project/data folder through NSOpenPanel. Its saved choice
is bound to the installation/device. Home, system, Heiwa state, and provider
credential roots cannot be selected as a broad workspace. Existing Work and
automation flows do not inherit owner native-tool grants. Tool availability does
not establish macOS permission; native enrollment and saved resource selections
remain separate requirements.

The desktop folder dialog belongs to Heiwa's own process through the official
Tauri dialog plugin. Only the native command receives its selected path; the
webview supplies a conversation ID, never an arbitrary grant path. The matching
bundled CLI validates and atomically saves the same installation/device grant.
The generic authenticated write proxy cannot grant a folder. Inference choice
survives relaunch; unreadable or invalid preference storage falls back to the
on-device policy. Native helper discovery resolves a CLI symlink before locating
its matching resource binary.

Mutating model tools use the operator approval surface, even when the runtime
was launched from a CLI. The trusted, explicit `heiwa tools run` command retains
the established owner-CLI policy. Unknown tool effects default to high risk.
New artifacts are an intentionally low-risk exception: visible basename only,
private UTF-8 file, no overwrite, read-back/hash verification.

## Supported transports

Codex uses the supported stdio app-server and its existing provider-owned
ChatGPT login. It checks ChatGPT account mode and the built-in OpenAI provider.
The integration never reads authentication tokens or extracts private app
endpoints. An explicit connection test must complete a host-tool nonce challenge
before its exact model enters the registry. A catalog entry alone is not proof
of entitlement. An unsupported configured default was observed during device
work; the successful model is selected from the CLI catalog and actually tested.

Each child starts in an empty directory. Native shell, browser, apps, plugins,
search, delegated workers, and inherited MCP servers are disabled. Empty config
tables can merge, so discovered MCP/plugin entries are explicitly disabled.
Only Heiwa dynamic tools are admitted. Unsupported native action/approval
requests fail closed. Repeated call IDs reuse the prior result. Prior chat
messages are injected as role-bearing Responses items using the installed
protocol's `thread/inject_items` method. Project/skill operator instructions are
disabled for the child. Provider-owned managed policy still applies.

Ollama uses `/api/chat`: roles, tool schemas, assistant tool calls, tool-role
results, usage, completion/error frames, and cancellation. Thinking is not
rendered as assistant text. HTTP/JSONL input is bounded; incomplete and length-
limited responses cannot claim success. Chat discovery excludes embedding-only
models even when their metadata also advertises tools/thinking. Admission binds
the effective configured endpoint. Only loopback endpoints qualify as on-device;
a remote Ollama endpoint cannot satisfy local-only privacy or a local-zero-cost
claim. No proxy is inferred from the process environment.

Claude Code and Gemini CLI have no equivalent governed host-tool bridge here;
agentic turns refuse that CLI path instead of invoking unobserved native tools.
Their direct API connections can use the compatibility tool protocol.

## Native tool scope

| Surface | Implemented in this change | Remaining scope |
| --- | --- | --- |
| Calendar | selected reads, range queries, approval-staged creation through existing services | in-turn write completion, general edits/cancellation/recurrence |
| Reminders | selected-list reads; owner-approved create and completion changes; fresh-store read-back; exact expected title on updates | recurrence, moves, deletion, Work-worker write admission |
| Notes | title search, folder inventory, exact-ID reads, approved new notes with text verification | existing-note edits, attachments, live watching, resource-selection UX |
| Files/Finder | real workspace inventory, native directory choice, bounded reads, new verified artifacts, approved reveal | moves/revisions, full file tree, security-scoped bookmarks for a sandboxed distribution |
| Terminal/processes | approved directory handoff; argv-based workspace execution under macOS Seatbelt, no network, cleared environment, deadline/output bounds | interactive Terminal observation, long-lived jobs, other OS sandboxes |
| Mail | bounded existing header snapshot workflow | live bridge timeout diagnosis, bodies/drafts/sending |
| Other Apple apps | explicit unsupported boundaries remain | Contacts, Photos, Messages, Safari, Shortcuts, iWork, Music |

The Reminders R1 read/propose command contract remains. New effects belong to
the owner tool lane; this is not completion of the separate C1-b worker authority
and containment program.

`heiwa tools list`, `show`, and `run` share the same catalog as chat. `--args-file`
packages reusable inputs/command argv without repeatedly pasting long scripts.
The Apple Notes JXA program and native helper are shipped implementations, not
model-generated scripts on every invocation. Existing `heiwa workflows` commands
remain bounded deterministic source aggregations. CLI effects write private
intent and completion receipts. CLI output uses `heiwa.cli/v1`.

## Presentation and truth

The app exposes inference choice, live actions, approvals, cancellation, errors,
and runtime-verified result cards. Conversation context comes from the admitted
operator projection, survives restart, and excludes the current/other threads.
A passing native action is distinct from model narration: a local model was
observed to misstate a filename after correct tool execution. Saved file paths,
hashes, native IDs, and verified outcomes therefore come from runtime receipts.
This is execution evidence, not a claim of frontier model quality or full parity
with proprietary desktop assistants and their tools.

## Verification and limits

The CLI sweep found and removed a second, unbounded Apple Calendar script
scanner. `calendar sync` now uses the same saved selection, bounded EventKit
reader, and reconciliation as the desktop. Missing selection never falls back
to all calendars. Google retains its separate limit/days import parameters.

Regressions cover the public admitted adapter (including host-tool forwarding),
role/history transport, duplicate call IDs, failed/incomplete/malformed streams,
provider-native action rejection, cancellation of launcher descendants, multi-
round execution, scope/symlink and artifact overwrite denial, actual macOS
sandbox file/network boundaries, malformed native effects before permissions,
canonical history replay, CLI contracts, and UI approval/stop/result behavior.
Live device tests must additionally bind the matching installed bundle and
verify native persisted state. Unit tests alone are not that acceptance.

No public promotion/release is implied. Mail remains a concrete live blocker.
Its read still timed out with Automation and Full Disk Access enabled. A first
timeout is now reported as a timeout, rather than inferred permission denial or
pending consent. No Mail body/send capability is inferred from these grants.
The CLI app-server dynamic-tool interface is experimental and version-sensitive;
unknown methods/features must produce a visible failure, never a silent legacy
execution fallback.

Installed-app acceptance caught a GUI-only interpreter failure: finding an npm
launcher does not put Node on a macOS app's minimal PATH. Provider children now
use discovery's explicit binary directories, preserving the caller's priority
and sandbox overrides without sourcing interactive shell configuration.

Primary references:

- [Codex app-server](https://learn.chatgpt.com/docs/app-server)
- [Sign in with ChatGPT for eligible apps](https://developers.openai.com/cookbook/articles/sign-in-with-chatgpt)
- [Ollama chat API](https://docs.ollama.com/api/chat)
