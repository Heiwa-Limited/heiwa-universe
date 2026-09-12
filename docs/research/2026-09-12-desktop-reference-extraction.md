# Backend capabilities that improve Heiwa's desktop

Research date: 2026-09-12. Classification: Intake, Execution, Evidence.

## Decision

Prioritize OpenCode for the Solid application, Goose for Rust execution and
approval state, Wave Terminal for workspace objects and previews, pi for provider
content/effort normalization and session context, and AG-UI for presentation-event
contracts. Keep Try Omarchy as the native lifecycle reference. These are sources
of specific mechanisms and tests, not proposed replacements for Heiwa's runtime.

The frontend needs better information and actions from the backend: identifiable
work, typed content, selected-resource context, recoverable execution, and useful
artifacts. It also needs frontend engineering: stable keyed updates, bounded
history, preserved scroll position, keyboard navigation, and accessible controls.
Backend progress alone does not create a good interface.

## Evidence boundary

Read repository READMEs before inspecting the source paths below. Shallow sparse
source mirrors are local research material under the Codex task's `work/repos`.
Compared with Heiwa `719c6bb4239bcd4c150d856875e4e536f9a1dfeb`, confirmed as remote
`dev` at the start of the review. The main checkout was behind that revision and
had unrelated operator configuration changes; implementation uses a separate
worktree, `codex/desktop-lifecycle-research`.

At final verification, remote `dev` had advanced to
`feb311d746edfe3bab0a897828f888327fe6f54f`. The intervening changes concern
installed-runtime shutdown and installation evidence; they do not touch this
patch's files. This patch remains based and tested on `719c6bb`.

This is source inspection, not a benchmark or installation certification of the
reference applications. Their complete test suites and live integrations were
not run. License labels below are from their root license files; an actual code
import still needs the selected files' notices and dependency provenance.

## Ranked references

### Original reference: Try Omarchy

Revision: `12d7c8a6621f3e6fd59ddb0632dfbbd987e1c751`.

The [host sleep controller](https://github.com/omacom/try-omarchy/blob/12d7c8a6621f3e6fd59ddb0632dfbbd987e1c751/macos/Sources/OmarchyVMHelper/VMHostSleepController.swift)
tracks ownership of a pause, retains an ambiguous outcome after a lost command
acknowledgement, and bounds wake retries. Its
[run lifecycle](https://github.com/omacom/try-omarchy/blob/12d7c8a6621f3e6fd59ddb0632dfbbd987e1c751/macos/Sources/OmarchyVMHelper/VMRunLifecycle.swift)
distinguishes intentional stops from startup failures. Its
[build cache](https://github.com/omacom/try-omarchy/blob/12d7c8a6621f3e6fd59ddb0632dfbbd987e1c751/scripts/build-cache.py)
refuses to record success when inputs changed during a build.

Heiwa should adapt these ownership and evidence rules within its existing Rust
runtime. The repositories below contribute more directly to conversation,
composer, workspace, provider, and artifact capabilities.

### 1. OpenCode — closest application architecture fit

Revision: `95daf90670b7c039c436c85537da5fbfe2205b41`. Root license: MIT.

- [Session event reducer](https://github.com/anomalyco/opencode/blob/95daf90670b7c039c436c85537da5fbfe2205b41/packages/app/src/context/global-sync/event-reducer.ts): keyed Solid reconciliation of messages, content parts, tool state, diffs, and todos. It updates affected records rather than replacing the entire application state on every token.
- [Session cache](https://github.com/anomalyco/opencode/blob/95daf90670b7c039c436c85537da5fbfe2205b41/packages/app/src/context/global-sync/session-cache.ts): bounded caches and explicit preservation/eviction policy.
- [Request construction](https://github.com/anomalyco/opencode/blob/95daf90670b7c039c436c85537da5fbfe2205b41/packages/app/src/components/prompt-input/build-request-parts.ts): context files, selected line ranges, comments, images, and agent references become identified request parts.
- [Permission service](https://github.com/anomalyco/opencode/blob/95daf90670b7c039c436c85537da5fbfe2205b41/packages/core/src/permission.ts): action/resource rules, explicit requests and replies, and cancellation of pending requests when the service scope ends.

Heiwa fit: its Solid UI can adopt the state-update and context-builder patterns
with little conceptual translation. Keep execution and policy in Rust. OpenCode's
pending permission map is not evidence of durable approval recovery across a
process restart.

Visible outcome: fast session switching, an attachment-aware composer, selected
objects that actually reach the model, and accurate inline tool cards.

### 2. Goose — strongest Rust execution reference in this set

Revision: `50666ae0b9a51e260b52b7efbab2e4e020346e94`. Root license: Apache-2.0.
The original `block/goose` URL now points to `aaif-goose/goose`.

- [Session effect application](https://github.com/aaif-goose/goose/blob/50666ae0b9a51e260b52b7efbab2e4e020346e94/crates/goose/src/agents/state_machine/session.rs): persists effects before publishing tool-confirmation messages, because an immediate user response must find durable state already present.
- [Tool approval operation](https://github.com/aaif-goose/goose/blob/50666ae0b9a51e260b52b7efbab2e4e020346e94/crates/goose/src/agents/state_machine/ops_tool_approval.rs): approval becomes an execution operation with structured effects rather than an isolated dialog.
- [Message content](https://github.com/aaif-goose/goose/blob/50666ae0b9a51e260b52b7efbab2e4e020346e94/crates/goose-provider-types/src/conversation/message.rs): separates text, images, tool requests/results, confirmations, and provider-supplied reasoning representations.

Heiwa fit: extend the existing operator events, Action Gate, provider adapters,
and evidence services. Adopt persistence-before-notification and typed effect
boundaries. Do not copy Goose's Auto/SmartApprove policy: its
[inspector](https://github.com/aaif-goose/goose/blob/50666ae0b9a51e260b52b7efbab2e4e020346e94/crates/goose/src/permission/permission_inspector.rs)
can allow tools from mode, annotations, or model-based classification. Those are
different authority assumptions from Heiwa's scoped leases.

Visible outcome: approval cards survive navigation and reflect real operations;
tool progress, cancellation, failures, and results share one execution history.

### 3. Wave Terminal — workspace and artifact interaction

Revision: `a4447c1563b2df285ab89e76c82f91e1a1a49c1e`. Root license: Apache-2.0.

- [Object identity](https://github.com/wavetermdev/waveterm/blob/a4447c1563b2df285ab89e76c82f91e1a1a49c1e/pkg/waveobj/waveobj.go): typed object references and object versions.
- [Block controllers](https://github.com/wavetermdev/waveterm/blob/a4447c1563b2df285ab89e76c82f91e1a1a49c1e/pkg/blockcontroller/blockcontroller.go): independent controller lifecycle, runtime status, and per-block resynchronization.
- [Preview model](https://github.com/wavetermdev/waveterm/blob/a4447c1563b2df285ab89e76c82f91e1a1a49c1e/frontend/app/view/preview/preview-model.tsx): file-type-specific preview behavior, file size limits, navigation, and remote file context behind a common view model.

Heiwa fit: use stable Work/artifact/resource IDs behind panes. Opening, splitting,
or focusing a view should not transfer ownership of its underlying work. Borrow
the interaction model rather than its Go/Electron implementation or unrestricted
file-access assumptions.

Visible outcome: edit a document beside its chat, inspect a generated image or
table, and keep useful results open independently of the session transcript.

### 4. pi-mono — provider content, effort, and conversation context

Revision: `71dca871bc80b6bc97be37f0ca3189399d651fff`. Root license: MIT.

- [Provider types](https://github.com/badlogic/pi-mono/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/ai/src/types.ts): typed text/image/tool content, usage, stop reasons, normalized thinking levels, and provider-native effort metadata.
- [OpenAI request translation](https://github.com/badlogic/pi-mono/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/ai/src/api/openai-responses.ts): maps requested effort into the actual provider request.
- [Session manager](https://github.com/badlogic/pi-mono/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/session-manager.ts): identified JSONL entries, parent links, branches, compaction, and context reconstruction.
- [Agent continuation](https://github.com/badlogic/pi-mono/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/agent/src/agent-loop.ts): continuation uses existing conversation context without adding another user message.

Heiwa fit: separate the durable transcript from the bounded provider context;
translate modality and effort at each adapter boundary and retain actual usage.
Do not import a second session authority. The README explicitly states that pi
does not include a built-in process/filesystem/network permission boundary.

Visible outcome: conversations can change eligible models without losing their
artifacts or history; image inputs and effort decisions become real capabilities.
These mechanisms do not themselves provide Heiwa's ROI routing or prove savings.

### 5. AG-UI — useful presentation protocol vocabulary

Revision: `bc17b0a539cc0da2e41407059f8e6d5a0d86d28e`. Root license: MIT.

- [Event schemas](https://github.com/ag-ui-protocol/ag-ui/blob/bc17b0a539cc0da2e41407059f8e6d5a0d86d28e/sdks/typescript/packages/core/src/events.ts): distinct message, tool, state, activity, run, and subagent events.
- [State application](https://github.com/ag-ui-protocol/ag-ui/blob/bc17b0a539cc0da2e41407059f8e6d5a0d86d28e/sdks/typescript/packages/client/src/apply/default.ts): full snapshots and validated JSON Patch updates, with separate activity and tool handling.
- [Event verification](https://github.com/ag-ui-protocol/ag-ui/blob/bc17b0a539cc0da2e41407059f8e6d5a0d86d28e/sdks/typescript/packages/client/src/verify/verify.ts): tracks entity ownership and lifecycle ordering, including subagent attribution.

Heiwa fit: consider an adapter over existing typed operator/Work projections if
interoperability is needed. Do not replace canonical JSONL or the Action Gate.
The inspected patch-error path logs state and leaves recovery to other code;
Heiwa needs redacted diagnostics and explicit revision-gap resynchronization.
Presentation events neither authorize tools nor guarantee exactly-once effects.

Visible outcome: rich progress and artifact cards can consume one predictable
contract instead of each screen interpreting provider text independently.

## Additional candidate: PI-Desktop

Revision: `d0dadd7df7eb83ec2af630749c1abc0ab61cb456`. Root license: LGPL-3.0.
Early Preview according to its README.

Its [session runtime](https://github.com/vastsa/PI-Desktop/blob/d0dadd7df7eb83ec2af630749c1abc0ab61cb456/apps/desktop/src/stores/runtime/session-runtime.ts)
has transcript caching, shared in-flight loads, and session configuration staging.
Its [permission manager](https://github.com/vastsa/PI-Desktop/blob/d0dadd7df7eb83ec2af630749c1abc0ab61cb456/crates/host-core/src/permissions.rs)
lets a late-attaching client read pending requests and bounds string previews.
However, pending requests are held in memory, and its tool-risk classification
labels `mcp_` names low-risk. That is not a policy to import into Heiwa.
Its [artifact table](https://github.com/vastsa/PI-Desktop/blob/d0dadd7df7eb83ec2af630749c1abc0ab61cb456/crates/host-core/src/artifacts.rs)
records edited paths, not proof that an output meets acceptance criteria.
Keep it as secondary product research; the five references above provide more
focused extraction targets.

## Heiwa: current code versus required capabilities

| Current source evidence | Backend or frontend work needed | User-visible result |
| --- | --- | --- |
| `operator/store.ts` deep-clones snapshots; `state/operator.ts` coalesces publishes; `Conversation.tsx` renders message bodies and always follows the tail | Stable keyed message/content projections, bounded history, selective updates, intentional scroll-follow behavior | Smooth long conversations with expandable tools and stable reading position |
| `heiwa_provider::adapter::Message` still has `content: String`; operator state has artifact records without a rich conversation renderer | Versioned content parts, attachment admission/storage, provider translation, artifact preview/export, exact producing-tool receipts | Paste an image, attach a document, generate/edit a useful artifact inside Heiwa |
| Composer displays a viewing caption but submits a prompt and automatic route policy | Typed per-window selected-object context, source IDs/revisions, bounded excerpts, inspection/removal before submission | "Explain this event" or "reply to this message" refers to the selected object |
| Passive onboarding catalog distinguishes detected apps, tools, and account registration; broader resource actions remain connector-specific | Supported native/account connection flows, scope selection, freshness/revocation, shared effect services | Calendar/Mail/Files surfaces become working resources, not informational cards |
| Existing native stream has bounded retries and authenticated cursors; existing client supports history replay and generation checks | Explicit recovery after retry exhaustion; preserve unknown submission outcomes; finish native host wake and active-work exit policy | Interrupted connections recover without repeated prompts or tool actions |

The existing macOS 27 product contract and Work Fabric ledger remain sequencing
authorities. This research does not mark pending A1-c3/restart-recovery acceptance
complete and does not supersede the accepted provider-truth design.

## Recommended order and acceptance

1. **Recoverable work and observations.** Finish shared Work/revision projections and lifecycle recovery. Prove two windows observe one run; closing an observer does not cancel it; wake/reconnect reconciles missed events; unknown effects are never replayed automatically.
2. **One attachment-to-artifact workflow.** Add typed input/output parts through the Rust/provider boundary. Prove picker/paste, size/type rejection, provider admission, actual output, preview/export, cancellation, and restart under a fresh profile.
3. **Context-aware productivity.** Give the permanent composer selected-object context. Prove a Calendar/Mail object retains source identity and authorization through a staged write and persisted readback.
4. **Efficient rendering and workspace interaction.** Adapt keyed Solid projections and independently addressable previews. Measure update cost on long transcripts; verify scroll, focus, keyboard use, and view disposal while work continues.
5. **Measured routing.** Use exact channel/model/effort/modality evidence and observed outcomes. Compare accepted-result cost and latency; keep unknown usage/cost unknown and do not equate a model field with functioning routing.

Each item must deliver an end-to-end user outcome. Avoid completing an oversized
backend rewrite before any useful interaction reaches the desktop. Conversely,
do not represent a capability as working merely by adding a button or card.

## Implementation in this branch

First Omarchy-inspired lifecycle extraction:

- Explicit conversation reconnect performs history replay and a new observation;
  concurrent reconnect clicks share one attempt. It never resubmits a prompt.
- Generation checks discard stale frames and disposed recoveries. The UI shows
  a recovery control even when an existing transcript is visible.
- Runtime startup observes its child before trusting a late listener, reporting
  an already-exited child rather than claiming it is ready.
- A window releases its own subscriptions; the owned runtime is stopped on
  application exit rather than when any individual window is destroyed.

This does not implement native macOS sleep notifications, suspension of provider
work, automatic retry of mutations, or restart certification. Those need the
runtime-owned active-work/reconciliation policy and separate acceptance tests.
No reference implementation was copied into Heiwa; the branch adapts mechanisms
within its existing services. Validation receipts accompany the task output.
