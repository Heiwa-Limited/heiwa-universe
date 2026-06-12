# Capability Fabric

**Status:** Product material for the modular Heiwa runtime.

Heiwa's target is a single input layer for all user-authorized digital action.
The product should feel intuitive because the hard parts are represented as
typed capabilities, trusted connectors, leases, and evidence, not because a
single prompt pretends to understand the user's whole life.

## Core Model

Every useful thing Heiwa can do is materialized as one of these resources:

| Material | Examples | Heiwa representation |
| --- | --- | --- |
| Accounts | Apple, Google, Microsoft, GitHub, Discord, OpenAI, Anthropic | account connector plus auth mode |
| Data | files, mail, calendar, contacts, repos, issues, messages, memories | indexed resource with source, owner, freshness, and sensitivity |
| Tools | shell, browser, MCP, local apps, APIs, webhooks | tool lease with risk class and action schema |
| Models | Ollama, Codex, Claude Code, Gemini CLI, APIs | provider adapter with capability, cost, privacy, and quota metadata |
| Devices | MacBook, workstation, WSL, remote runner, mobile bridge | device record with locality, trust tier, and available tools |
| Agents | provider agents, local loops, computer-use workers, review workers | worker lease with role, authority, budget, and evidence stream |
| Policies | approvals, data boundaries, budgets, retention, org rules | reducer/policy record enforced before execution |
| Evidence | receipts, transcripts, artifacts, diffs, screenshots, logs | append-only run evidence with source and verifier |
| References | official docs, OSS repos, SDKs, specs, examples, model cards | source pack with authority, license, freshness, risk, and promotion path |

The user gives one intent. Heiwa turns it into capability-aware work.

```mermaid
flowchart LR
    input["Single input layer"] --> classify["Intent + risk + privacy classifier"]
    classify --> graph["Capability graph"]
    graph --> plan["Execution plan"]
    plan --> leases["Tool / account / agent leases"]
    leases --> workers["Subagents + providers + containers"]
    workers --> evidence["Receipts + artifacts + status"]
    evidence --> user["Clear user updates"]
    evidence --> state["SpacetimeDB state/evidence"]
```

## Connector Contract

Each external account connector must ship as a small, portable module.

Required files or equivalent declarations:

| Contract | Purpose |
| --- | --- |
| connector manifest | provider name, account type, auth modes, scopes, APIs, rate limits |
| resource map | what data can be read, watched, searched, created, updated, deleted |
| action schemas | typed operations with inputs, outputs, side effects, and risk class |
| auth implementation | OAuth, device code, API key, local bridge, webhook, or WebSocket setup |
| secret boundary | where tokens live, how refresh works, how revocation is handled |
| sync strategy | polling, webhook, push subscription, local filesystem watch, or manual refresh |
| evidence hooks | receipts for reads, writes, approvals, failures, and external IDs |
| tests | offline contract tests plus live smoke tests behind explicit credentials |

No connector is product-grade until it can authenticate, list real resources,
execute at least one bounded useful action, record evidence, and revoke access.

## Source Pack Contract

Heiwa must be able to learn from official sources and OSS repositories without
turning every reference into a privileged integration. A source pack is read-only
Intake/Evidence material until promoted.

Required source-pack fields:

| Field | Purpose |
| --- | --- |
| source id | stable name such as `official.openai.agents-sdk.tools` |
| authority | official, official OSS, community OSS, internal, or user-private |
| source locator | URL, GitHub repo, local mirror path, package name, or docs index |
| ingest mode | web snapshot, Git mirror, package metadata, local file, or manual note |
| license / terms | public license, docs terms, unknown, or private-use-only |
| freshness | fetched timestamp, version, release tag, commit SHA, or stale marker |
| capability map | which tools, schemas, APIs, models, or examples it teaches Heiwa |
| risk tier | T0 reference, T1 draft helper, T2 staged tool, T3 external side effect |
| evidence ref | receipt, source URL, file path, commit, or checksum proving the ingest |

Promotion path:

1. **Reference pack** — readable docs/code/examples only.
2. **Capability manifest** — extracted capabilities with schemas and risk
   labels, still not executable.
3. **Adapter or connector** — code can read/list resources under policy.
4. **Tool lease** — execution allowed only through scoped leases and approvals.
5. **Product-grade integration** — auth, revocation, tests, receipts, and
   rollback/undo posture are proven.

This applies equally to OpenAI, Anthropic, Google, Ollama, GitHub, Rust, Python,
TypeScript, SpacetimeDB, WebAssembly, and future OSS/SOTA runtimes. The source
pack may be broad; the executable integration must stay narrow.

## First Connector Lanes

| Lane | Examples | First useful actions |
| --- | --- | --- |
| Apple | iCloud, Calendar, Contacts, Reminders, Mail, Messages via safe local bridge | create reminder, inspect calendar, draft message, file lookup |
| Google | Gmail, Calendar, Drive, Docs, Sheets, YouTube, Google AI | search mail, summarize thread, create calendar block, update doc |
| Microsoft | Outlook, Calendar, OneDrive, Teams, Office, Azure identity | triage inbox, create meeting, update document, inspect tenant state |
| GitHub | repos, issues, PRs, Actions, Releases, code search | open PR, fix CI, release artifact, issue triage |
| Messaging | Discord, iMessage bridge, Slack later | receive intent, request approval, send summarized result |
| Computer use | browser, desktop apps, local files, remote sandboxes | navigate, extract, edit, screenshot, verify result |
| Model/provider | Ollama, Claude Code, Codex, Gemini, Antigravity, direct APIs | route model work, delegate agent tasks, stream evidence |

Apple/iMessage integration should start local and consent-heavy. Apple Calendar should use a device-local EventKit bridge; Apple Mail should start with metadata/draft safety and only later earn MailKit extension depth. Google Calendar/Gmail should use narrow OAuth scopes, local read models, staged writes, and receipts. Google Calendar sync should preserve per-calendar sync tokens; Gmail should default to local pull/scheduled scan because Heiwa does not provide a hosted webhook runtime.

Google, Microsoft, and GitHub should start with OAuth scopes that are narrow enough to explain to a normal user.

## Subagent Delegation

Heiwa should coordinate specialized workers instead of becoming one large agent.

Worker classes:

| Worker class | Owns | Reports |
| --- | --- | --- |
| planner | decomposes task into typed steps and required capabilities | plan, assumptions, missing material |
| connector worker | performs account/API/resource operations | external IDs, outputs, failures |
| computer-use worker | uses browser/desktop/container surfaces | screenshots, DOM/state, action trace |
| model worker | runs provider/local model calls | prompt class, model, cost, output |
| verifier | checks result against tests, UI, API, or source evidence | pass/fail and residual risk |
| reporter | compresses state into user-facing updates | concise status and next action |

All workers operate through leases. A lease names the account, tool, scope, risk,
budget, expiry, and approval state. Workers report to Heiwa by writing events and
evidence, not by passing around unstructured chat summaries.

## User Update Contract

The user should not see internal noise. They should see:

- what Heiwa is doing now
- what account/tool/model is being used
- whether approval is needed
- what changed
- what failed and why
- where the receipt/artifact lives

Status events should be structured first and phrased second. The runtime should
be able to render the same event to CLI, Heiwa.app, Discord, or iMessage without
rewriting the task.

## Security Shape

Heiwa's trust model is capability-first:

- least-privilege scopes by default
- local Keychain or equivalent for raw user secrets
- SpacetimeDB stores metadata, references, leases, and evidence, not casual raw secrets
- short-lived leases for write-capable tools
- approval gates for money, messaging, deletion, publishing, production, and computer control
- revocation must be visible and testable per connector
- no ad tracking, behavioral resale, or silent cross-account profiling
- telemetry is local/off by default unless it is operational evidence the user can inspect

The product can be broad without being reckless. Breadth comes from modular
connectors and leases, not ambient access.

## Portability Rules

- Rust owns runtime authority, leases, provider supervision, and local execution.
- TypeScript owns companion visual clients and web/edge connector support where appropriate.
- Shell owns bootstrap and operator glue.
- Python remains compatibility/R&D unless a specific module earns product status.
- Each connector must have a manifest and tests before public claims.
- Each external dependency must justify itself with concrete user value.
- Generated code is acceptable only when reproducible from a source schema.
- Reference packs may be mirrored under ignored `repos/` or local Heiwa state,
  but runtime APIs expose only redacted manifests, counts, source refs, and
  evidence pointers by default.
- OSS repo code is never executable just because it was indexed. It becomes
  executable only after a connector/tool manifest defines its trust boundary.

## Runtime Modularity Targets

Heiwa should decompose capability execution by runtime fit:

| Runtime target | Heiwa role |
| --- | --- |
| Rust authority layer | local API, leases, provider supervision, resource admission, fast read models |
| TypeScript client contracts | Heiwa.app, typed cockpit clients, connector setup UX, generated bindings |
| Shell bootstrap glue | install, update, doctor, provider CLI resolution, operator probes |
| Python compatibility workers | document/R&D tasks and existing Python package compatibility when isolated |
| SpacetimeDB reducers/clients | deterministic sync, subscriptions, evidence, type-safe cross-device state |
| WebAssembly plugin sandbox | portable low-level modules with embedder-controlled imports |
| Ollama/local model lane | cheap private inference, embeddings, summaries, local classification |
| Provider-owned agent runtimes | Codex, Claude Code, Gemini CLI, Antigravity, and future peers as delegated workers |

The performance target is not "everything in microseconds." Cached local read
models and routing metadata should be microsecond-class. Network, model, GUI, and
provider work should be asynchronous, leased, observable, and resource-gated.

## Value Gate

Code is true-value code when it does at least one of these:

- connects a real account safely
- exposes a useful resource with freshness and sensitivity metadata
- executes a bounded action with approval and rollback/undo where possible
- routes to a cheaper or better model/tool without losing evidence
- reduces context or token waste
- records a receipt that improves future execution
- makes the same capability portable across CLI, Heiwa.app, and messaging clients

Everything else is reference material until it proves value.
