# Heiwa Engines: Answer and Execute Across Subscriptions, APIs, and Local Models

Date: 2026-09-27
Status: Draft. Design approved by Devon in conversation on 2026-09-27; this written spec awaits his review.
Planes: Intake, Execution, Evidence
Implements: `docs/macos27-product-contract.md` rows "Provider setup/routing" and "Apple productivity", and its execution inspector; Work Fabric § Inference Federation and Release C
Preserves: `HEIWA.md`, the macOS 27 product contract, and the Work Fabric design and ledger. Where they conflict with this document, they win.
Inputs: the Claude Desktop design conversation, and the ChatGPT Desktop architecture review of the same date. The review is kept outside this repository; its disposition is recorded in § Review Disposition.

## Decision

A person installs Heiwa.app, connects whatever inference they have (a
subscription agent runtime, a direct API key, a local model), and uses all of it
from one app for two kinds of request:

- **Answer.** A question, search, or explanation. It runs as a lightweight
  conversation turn with bounded retrieval and the smallest sufficient route. It
  does not create Work, a worker, a worktree, or a planning loop.
- **Execute.** An outcome that needs changes, effects, sustained execution, or
  recovery. It runs as durable Work, with grants, a governed tool loop,
  approvals, and receipts.

Execution has two families that share one tool and effect service, one Action
Gate, one lifecycle vocabulary, and one Work timeline in the app and the CLI:

1. **Provider agent sessions.** A subscription runtime such as Claude Code runs
   its own agent loop, with Heiwa's tools attached and its permission prompts
   answered by Heiwa policy.
2. **Heiwa tool loop.** A model endpoint such as Ollama or a direct API has no
   loop of its own. Heiwa runs the loop: it offers the same tools, executes them
   under the worker's grant, and returns results to the model.

The first outcome is **Calendar context → proposed reminders → authorized
Reminders write → verified receipt**. It runs through the same Work in the
desktop and the CLI, proven on one subscription agent channel (Claude Code)
and one local model channel (Ollama through the Heiwa tool loop).

## Placement

This spec does not start a parallel roadmap.

- It implements the macOS 27 product contract's provider, routing, execution
  inspector, and Apple productivity requirements. Apple event-to-reminder is
  named in that contract's next-implementation order.
- In the Work Fabric it is **Release C, slice 1 (C1)**: the first Reminders
  outcome, with § Inference Federation as its routing contract.
- C1 also closes two A1 ledger gaps it depends on:
  - composer submissions without a `work_id`;
  - ungated raw worker launch, for the engines it admits.
- C1 runs ahead of Releases A2 and B, following the macOS contract's newer
  next-implementation order. Approving this spec approves that reorder.
- Progress is tracked as Release C rows in
  `docs/superpowers/ledgers/2026-08-22-work-fabric-task-ledger.md`.

## Roles and Authority

### Collaborators are outside the product

Devon, Claude Desktop, and ChatGPT/Codex Desktop are the engineering and
business workspace around Heiwa. They are not product principals, product
workers, or features of Heiwa.app. This spec touches them only where they use
the `heiwa` CLI as users do.

### Product principals

**The user** acts through an authenticated client:

- Heiwa.app, through the existing signed native request contract;
- the `heiwa` CLI, through the user's local runtime credential.

Possession of a user credential is a technical access boundary, not proof of
human intent or new authorization. Desktop collaborators carry the user's
existing task authorization; this product spec does not widen it. The product
must protect its user credential and decision store from workers. A client's
self-reported identity, such as the `CLAUDE_CODE_ENTRYPOINT` environment
variable, is recorded as an audit label
marked `self_reported`. It never grants authority.

**Workers** are engines that Heiwa launches for Work. A worker holds only an
explicit, revocable, expiring grant: Work, workspace scope, tools, budget, and
deadline. A worker may stage actions. It may not approve, widen its grant,
claim the user, or pass authority to child processes implicitly.

The shared decision service keeps its current role. The `local-cli` label that
the approval CLI passes today is an audit label, not authority. It is replaced
by the authenticated client principal.

### Enforcement is a gate, not a claim

A worktree is a separate checkout. It is not a sandbox. The current worker
spawn path sets a working directory and clears the environment, and the ledger
records raw worker launch as ungated. "A worker cannot approve itself" is true only
when the worker cannot obtain the user's credential or reach the user-authority
API.

- **Heiwa tool loop.** The model receives no direct host-process capability.
  Tool requests are untrusted inputs; the Rust tool service must validate the
  worker's grant, source scope, egress, budget, and effect policy on every call.
  This boundary needs denial tests. Running the loop in Rust alone does not
  establish containment or prevent an overly privileged tool from misusing
  the runtime's authority.
- **Provider agent sessions.** Admitted for execution only when an enforcement
  probe passes for the installed provider version on the user's machine. The
  probe proves that the worker's process tree cannot:
  - read Heiwa credentials or approval state under the Heiwa data root;
  - connect to the runtime's user-authority API;
  - send Apple Events to other applications;
  - spawn processes that escape the sandbox;
  - write outside its worktree.

  Network egress is limited to the admitted provider route and scoped tool
  transport. Required inference connectivity must work, while alternate paths
  to local authority and unauthorized disclosure must remain blocked. A profile
  that denies all networking is useful for an offline spike but cannot admit
  a cloud engine.

The mechanism is decided by C1's first task, an enforcement spike. Candidates
are the provider's own sandbox configuration pinned by Heiwa (Claude Code
sandbox settings, Codex sandbox policy) and a Heiwa Seatbelt profile where
nested sandboxing permits.

A route that fails the probe is shown as **not enforceable** and is not used
for C1 execution. An unrestricted host session would have to be a separately
designed user-authority mode, outside C1's governed worker promise. Withholding
Heiwa effect grants cannot prevent an unrestricted process from making the
same effects through native tools or reading the user's credentials.

The provider's Bash sandbox is not a process-wide sandbox: its built-in file
tools use separate permission checks, and integrations can execute outside the
shell sandbox. Admission must cover the complete launch and tool configuration,
not just a successful Bash denial. See the provider's
[sandbox scope](https://code.claude.com/docs/en/sandboxing#scope).

The reproducible offline spike and remaining integration proof are recorded in
`docs/superpowers/plans/2026-09-27-engines-c1a-enforcement.md`. Its canary results
cannot set an Engine to `enforceable`.

## Answer and Execute

### Answer path

- Runs as a conversation turn in the session's thread. No Work, worker,
  worktree, or planning loop.
- Uses bounded retrieval: Context Broker references and minimal context.
- Answers deterministic questions, such as "what's on Tuesday", from tools
  directly, with no model call.
- Uses the `answer` route policy. Eligible routes are local models that fit the
  device, and routes the user explicitly permitted for answers (API keys with a
  budget, free tiers).
- Excludes subscription agent pools from automatic answer routing by default.
  These include OpenAI Work/Codex, Gemini CLI/Code Assist, and the Claude
  subscription. Each pool has a per-pool "use for answers" switch.
- When no route is eligible, explains what to connect or permit. It never
  silently consumes a reserved pool or switches to paid API usage.
- Lets the user deliberately pick a pool for one answer, for example `@claude`.

### Execute path

Runs as durable Work with a grant, a workspace when files are involved, the
governed tool loop or a provider agent session, Action Gate effects, and
recovery.

### Promotion

A request is promoted from Answer to Execute when it needs effects, file
changes, sustained execution, or recovery. Bounded read-only search, fetching
sources, and Calendar lookup can use multiple tools while remaining Answer;
tool count alone is not the classification rule. Answer reads still pass the
same data-access and egress policy, with sources and usage recorded.

- Classification is deterministic rules first, then a local model. A paid call
  is never made only to classify, as `HEIWA.md` § Optimization Doctrine
  requires.
- The user can force either path ("just answer" or "do it").

Heiwa's `Work` record and a provider's billing category are unrelated. Creating
Work does not move usage into or out of any provider allowance.

## Allowance and Consumption

### Allowance pools

An allowance pool records where consumption lands. It is independent of model,
transport, and credential, and it extends `ProviderAccount.rate_group` rather
than adding a second account registry.

```text
AllowancePoolV1 {
  pool_id,
  provider, account, workspace?,
  billing_mode: subscription | metered | free_tier | local,
  shares_with: [pool_id],
  reset_window?: { kind, next_reset_at? },   // only when the provider reports it
  use_for: { answers: bool, execution: bool },
  evidence: { source, observed_at, truth_class },
}
```

`truth_class` is one of `measured`, `provider_reported`, `estimated`, or
`unavailable`.

C1 defaults for `use_for`:

| Pool type | Answers | Execution |
| --- | --- | --- |
| Local | on | on |
| Subscription | off | on |
| Metered API | off until the user permits it with a budget | off until the user permits it with a budget |
| Free tier | off until permitted | off until permitted |

Every answer and execution step records the pool it consumed and its usage,
each with a truth class. Unknown usage is displayed as unknown, never zero. A
cheaper model reduces consumption but does not change the pool.

### Documented provider boundaries (2026-09-27)

These are documentation findings, not measurements of any user's account. Plan,
workspace, and rollout can change effective limits.

| Route | Documented behavior | Consequence |
| --- | --- | --- |
| Claude subscription via Claude Code, `claude -p`, or Agent SDK | They draw from the same subscription usage limits as Claude chat. An announced separate Agent SDK credit is paused. Verified from the [provider notice](https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan) (page dated 2026-06-16). | Headless use is not a separate pool. Model it as one shared subscription pool. API-key billing is separate. |
| OpenAI subscription via Codex | ChatGPT Work and Codex share pricing, credits, and limits ([pricing](https://learn.chatgpt.com/docs/pricing)). | A short question through Codex consumes that pool. Excluded from automatic answers by default. |
| OpenAI direct API | Billed through the Platform account, including `chat-latest` ([API pricing](https://developers.openai.com/api/docs/pricing)). | Metered pool. No supported route that spends the ordinary ChatGPT Chat allowance has been established, so none is advertised. |
| Google | Limits are product-specific. Gemini CLI and Code Assist agent mode share quota. API-key and Google-account authentication differ ([CLI quotas](https://geminicli.com/docs/resources/quota-and-pricing/)). | Model app, CLI/Code Assist, and API as distinct routes. The Gemini app allowance is not an API balance. |

A route that spends ChatGPT's ordinary Chat allowance is advertised only if an
authorized, supported interface and its billing attribution are verified.

## Engines

An Engine is a verified, selectable execution configuration. It is a projection
of existing facts:

- provider, account, and channel;
- exact model and effort support;
- tools and modalities;
- allowance pool and device fit;
- authorization and enforcement status;
- recent execution evidence.

It does not own the user's Work, permissions, or evidence.

### Admission states

Each state is distinct and shown separately:

1. `discovered`
2. `connected`
3. `model_available`
4. `execution_verified`, reached only when the user runs a test or a real task
5. `enforceable`, for provider agent sessions only

Setup stays passive: no inference, app launch, or content read until the user
asks for a test. Installing a desktop app does not grant Heiwa that app's
subscription or tools.

### Version capability matrix

Each provider CLI is probed for the features Heiwa uses. Examples:

- Claude Code: `stream-json`, `--mcp-config`, `--permission-prompt-tool`,
  sandbox settings.
- Codex: `app-server` protocol types, `exec --json`, sandbox policy.
- Gemini CLI: `stream-json`.

A missing capability produces an explicit unsupported state, not a degraded
guess. Heiwa uses the user's installed CLI and never depends on
developer-only bundle paths, such as a Codex binary bundled inside another
application.

### Slice-1 engines

- **Claude Code**, as a provider agent session on the user's subscription. The
  development machine observed version 2.1.231.
- **Ollama**, as a local model through the Heiwa tool loop, for admitted models
  that support tool calling.

Codex follows in slice 2 through `app-server`, a bidirectional interface
with approvals, dynamic tools, resume, and turn-level effort and sandbox policy.
The review probed its schema in a Codex 0.158 alpha; the development machine's
installed CLI is 0.146.0, so it must be re-probed. Gemini CLI follows later.

## Execution Families

### Provider agent sessions (Claude Code in C1)

- Launch: `claude -p … --output-format stream-json --verbose`, plus
  `--mcp-config` for the worker's Heiwa tool server,
  `--permission-prompt-tool` pointed at the Heiwa permission tool, and the
  pinned sandbox settings. Each flag is used only when the capability matrix
  confirms it.
- The permission tool asks Rust policy:
  - inside the grant (for example, an edit inside the worktree) → allow;
  - otherwise → stage an approval and wait, up to a bounded timeout, then deny
    with a reason.
- The adapter translates `assistant` `tool_use` blocks, `user` `tool_result`
  blocks, text, usage, and the terminal `result` event into the lifecycle
  below.

### Heiwa tool loop (Ollama in C1)

- `heiwa_loop` becomes a tool-using loop:
  1. call the model with tool schemas;
  2. execute tool calls through the Rust tool service under the worker's grant;
  3. return correlated results;
  4. repeat until done, bounded by steps, budget, and time.
- The `ProviderAdapter` `send` contract gains tool definitions, correlated tool
  calls and results, rich content, effort, and scope. Today it carries text and
  token, tool, done, and error events only.
- There is one loop. It lives in Rust, not in the UI.

## Tools and Effects

- One Rust tool and effect service: the existing `heiwa_mcp` registry, which
  already implements scoped file read/list and repository grep, extended with
  Apple tools.
- MCP over stdio, one server per worker, scoped by the worker's grant, is a
  **transport into that service**, not a second permission system. Tool
  annotations are untrusted metadata.
- Slice-1 tools:
  - `calendar.read` (bounded; through the existing EventKit path);
  - `reminders.lists`;
  - `reminders.read`;
  - `reminders.propose`, which stages a `ReminderCreate` effect;
  - `work.note`.
- Effects execute only through the Action Gate. An approval binds the exact
  payload, target list, source revision, expiry, and idempotency key. Execution
  revalidates each of these, and an approval is consumed once.
- Reminders are written through the Swift EventKit helper. Each reminder
  carries a `heiwa://effect/<effect_id>` marker for reconciliation, following
  the calendar plan markers.

### Access, disclosure, and effect are separate decisions

- **Reading** the calendar is access.
- **Sending** calendar content to a cloud engine is disclosure. It requires
  permission for that destination.
- **Creating** a reminder is an effect.

Context items carry source ID, revision, sensitivity, and permitted
destinations. Derived summaries inherit their sources' sensitivity. Standing,
revocable grants cover routine bounded work so that harmless steps do not
interrupt the user. A local-only task explains when the machine lacks a capable
local route instead of falling back to cloud inference.

C1 defaults:

- `calendar.read` and `reminders.read` hold a standing grant for local engines.
- Disclosure of calendar or reminder content to a cloud engine asks once per
  source and destination. The user may make that answer standing and revoke
  it later.
- Every `ReminderCreate` needs an approval. That approval may come from the user
  or from the user's authenticated assistant, never from a worker.

## Lifecycle and Evidence

- **Action lifecycle.** `requested → authorized → started → succeeded | failed |
  cancelled | indeterminate`, with correlation IDs (`tool_call_id`,
  `effect_id`, `approval_id`). A tool request is not a completed action.
- **Terminal truth.** Success needs both the provider's terminal result and a
  verified process exit. A missing result is `indeterminate`. The Claude and
  Gemini adapters' synthetic `Done` at end of stream is removed.
- **Effects survive crashes.**
  - Intent is persisted before execution, and the result after.
  - After a crash between the two, the source app is reconciled by marker before
    any retry. Resume is not retry, and no duplicate reminders are created.
  - Consequential effects are read back: the reminder exists with the approved
    fields.
- **Protocol drift.** Unknown provider event types are counted by type name and
  surfaced as a drift warning. Raw frames are not stored. Diagnostics keep only
  policy-admitted, redacted payloads.
- **Event types.** Existing `OperatorEventType` payloads are extended. New types
  are added only where none fits: persisted effect intent, and effect
  reconciliation.

## Surfaces

### Desktop

- Composer submission inside a Work carries an explicit Work scope. Rust
  validates thread membership and binds the scope; the UI cannot mint authority
  by supplying an ID.
- **Work timeline:**
  - one lane per worker, with an engine badge showing provider, model,
    allowance pool, and enforcement status;
  - lifecycle rows;
  - inline approval cards showing the exact effect;
  - receipts.
- Answers render as ordinary conversation turns, with sources and pool
  attribution.
- Pending approvals raise a macOS notification through Tauri. Clicking it opens
  the card.
- Closing a panel ends observation only. Cancellation targets execution
  explicitly.

### CLI

- Uses the same runtime, the same Work, and the same policy.
- Structured results use a versioned envelope:
  `{schema: "heiwa.cli/v1", ok, data | error{code, message, hint}, next[]}`.
- Diagnostics go to stderr. Exit codes are stable and follow
  `docs/design/refs/CLI.md`; for example, `4` means approval required.
- Prompts appear only on a TTY, and nothing blocks a pipeline. A TTY is not
  authentication.
- Slice-1 verbs, all supporting `--json`:
  - `heiwa ask "<question>"`: the answer path. The existing one-turn `ask`
    verb becomes this path.
  - `heiwa run "<task>" [--engine <id>] [--detach]`: the execute path; creates
    Work and routes it to an engine. This is distinct from `heiwa work run`,
    which spawns a raw command. That verb stays an advanced capability behind
    the Action Gate.
  - `heiwa work watch <id> [--since <cursor>] [--json]`: a resumable event
    stream, NDJSON with `--json`.
  - `heiwa approvals list|show|decide`, on the new envelope.
- `heiwa help --json` is generated from the command registry, the single source
  shared with the app's command catalog.

## Success Measures

Report:

- accepted outcomes;
- attention required: approvals, interventions, and time to decision;
- latency: answer time-to-first-token and task completion;
- privacy exposure: items and bytes disclosed to cloud engines per Work, by
  source;
- consumption per allowance pool, with truth class.

Provider count and successful process launches are not success measures.

## Error Handling

| Condition | Behavior |
| --- | --- |
| No eligible answer route | Explain which connection or permission is missing; do not consume a reserved pool |
| Engine auth expired or account failure | Mark the Engine disconnected; re-route if eligible, else block with the remedy |
| Enforcement probe fails | Engine is `not enforceable`; governed execution refused; no unrestricted fallback in C1 |
| Permission wait timeout | Deny the provider request with a reason; the Work records a blocker |
| Malformed stream or missing terminal result | Step is `indeterminate`; the user sees it and may retry explicitly |
| Provider protocol drift | Count unknown types; drift warning on the Engine; continue if the terminal result is valid |
| Cancellation | Target the run; the provider process is stopped and verified stopped; pending approvals are withdrawn |
| macOS permission denied or revoked | Tool returns an explicit permission state with the system remedy; nothing is fabricated |
| Source changed after approval | Revalidation fails; the approval is invalidated and re-staged |
| Crash around an effect | Reconcile by marker before retry; record `succeeded` or `indeterminate`; never duplicate |
| Runtime restart with pending decision | Pending approval survives; the worker resumes waiting or is marked stale with explicit continuation |
| No capable local route for a local-only task | Explain the device limit; no silent cloud fallback |

## Verification

C1 is accepted when `scripts/check_engines_c1_acceptance.sh` passes on a fresh
per-user profile, and the tests below pass under the existing CI groups.

0. **Enforcement spike.** Prove or refute each boundary in § Enforcement for
   Claude Code (installed version and current release) on macOS 27. Record the
   mechanism chosen and the version matrix. Each denial requires a successful
   positive control; crashes, timeouts, missing files, and unavailable services
   are inconclusive. Policy queries are distinct from real effects. Bind later
   admission to the provider executable, effective configuration, sandbox
   profile, transport, and host evidence; invalidate it when those inputs change.
1. **Authority.**
   - Desktop and CLI submissions bind to durable Work.
   - A worker cannot approve itself or escape its scope. Tested attempts:
     direct API and CLI calls from inside a worker; reading credential or
     approval files; Apple Events to Reminders and Terminal; detached spawns;
     grant expansion.
2. **Both families.** Claude Code and the Ollama tool loop both reach the Rust
   tool service and the Apple effect service, with version-matrix admission and
   a real approval response path.
3. **Decisions and failures.**
   - Approved, denied, revoked, expired, and stale-source decisions.
   - Malformed streams, missing terminal results, cancellation, account failure,
     and missing local capacity.
   - Adapter tests replay recorded provider streams that include tool use and
     permission prompts.
4. **Restart.** Restart during a pending decision and around a Reminders write.
   The same Work recovers, uncertain effects reconcile without duplicates,
   Reminders state is verified, and both clients show the same receipt.
5. **Fresh profile.** No repository assumptions. Also covers denied macOS
   permission, no connected provider, and an explicit data-egress choice.

Local acceptance does not prove a public release. Signed and notarized
clean-machine installation evidence remains a separate delivery requirement.

## Delivery Order

C1 is delivered as sub-plans. Each sub-plan leaves `dev` green and its ledger
rows verified.

| Sub-plan | Scope | Depends on |
| --- | --- | --- |
| C1-a Authority | Enforcement spike; authenticated CLI principal replacing `local-cli`; worker grants; Work-scoped desktop and CLI submission | — |
| C1-b Tools and effects | MCP stdio transport into `heiwa_mcp`; calendar and reminders tools; EventKit Reminders write with markers; persisted effect intent; reconciliation | C1-a |
| C1-c Engines | Capability matrix and admission states; Claude Code session with permission tool; `ProviderAdapter` tool contract; Ollama tool loop; lifecycle states; no synthetic `Done` | C1-a, C1-b |
| C1-d Surfaces and routing | Answer path and allowance pools; Work timeline, approval cards, notifications; CLI envelope, `ask`, `run`, `work watch`, `help --json` | C1-c |
| C1-e Acceptance | `scripts/check_engines_c1_acceptance.sh` on a fresh profile; restart and reconciliation proofs | C1-d |

## Later Slices

| Slice | Scope |
| --- | --- |
| C2 | Codex through `app-server`; Gemini CLI |
| C3 | Mail drafts, Notes, Files (security-scoped bookmarks), Contacts lookup |
| C4 | Apple Foundation Models and MLX local engines; Shortcuts and App Intents entry points |
| C5 | Independent cross-engine review; measured quality fed back into routing |

## Review Disposition

The ChatGPT Desktop review (2026-09-27) proposed five amendments. All are
adopted:

- separate collaborators, principals, and workers;
- app and CLI as clients of one durable Work;
- explicit execution capabilities;
- access, disclosure, and effects as separate decisions;
- success and cost as evidence.

Its lightweight answer path and allowance-pool model are adopted too, along
with two factual corrections: Gemini already uses `stream-json`, and `heiwa_mcp`
already has a scoped registry with file and grep tools.

Changes from the original conversation design:

- two engines for slice 1 instead of four;
- no new release label (C1 instead of "A3");
- enforcement is an admission gate rather than an assertion;
- unknown provider events are counted, not stored.

**Independently checked at `origin/dev` `5de52e1b`:**

- desktop `submitTurn` carries no `work_id`;
- worker spawn sets a working directory and clears its environment, without an
  enforced sandbox;
- the ledger records worker launch as ungated;
- approval decisions use the `local-cli` label;
- Gemini uses `stream-json`;
- the Claude adapter synthesizes `Done` at end of stream;
- the Claude subscription notice.

**Cited from the review, not independently checked:** the OpenAI and Google
documentation rows, and the Codex `app-server` schema probe.
