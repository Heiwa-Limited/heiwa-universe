# HEIWA

Updated: 2026-05-22
Status: Canonical truth for `heiwa-universe`

This file replaces the old repo-root compatibility shim. When `README.md`, legacy plans, or older architecture notes conflict with this document, this document wins.

## One-Sentence Truth

Heiwa is the operating layer that turns one user intent into governed, routed, verified, multi-tool AI execution.

Current shape:

- `heiwa` is the installed runtime and CLI control surface.
- `Heiwa.app` is the installed primary user input/display surface.
- DREX is the internal execution kernel.
- GitHub is the source and install authority. The installed local runtime plus
  `~/.heiwa/` are the current user-functionality truth on each machine.
- SpacetimeDB is the adjudication, subscription, and evidence sync plane when enabled.
- Rust proposes and executes.
- Providers still own their own inference internals.
- Heiwa turns the user's local models and connected providers into one coherent operator experience.

## Interaction Contract

The user should experience Heiwa as one conversation, not a collection of tools
they must manually coordinate.

Primary loop:

1. The user asks, answers, approves, or corrects Heiwa in one input/output
   thread.
2. Heiwa watches connected surfaces in the background: browser, mail, calendar,
   messages, forums, files, machines, provider CLIs, local models, computer-use
   surfaces, GitHub, Cloudflare, STDB, and other approved integrations.
3. Heiwa compresses that background state into context the user can understand:
   what changed, why it matters, what Heiwa is doing, what needs approval, and
   what evidence exists.
4. Safe work can proceed in the background. Risky writes become staged actions
   with target, payload, cost/risk, and expected receipt.
5. Results, blockers, receipts, and follow-up questions return through the same
   thread and are inspectable in the app/runtime state.

Heiwa.app is the everyday executable client/display for this loop. Normal
installs must place the local runtime under `~/.heiwa/` and the user-facing app
under `~/.heiwa/app/Heiwa.app` or the platform-equivalent HOME-local app path.
It may show panels for Inbox, Providers, History, Traces, Memory, Approvals,
Status, and other surfaces, but those panels are inspectors over the same
runtime truth. They do not replace the single Heiwa/user conversation.

The browser console is a per-user pseudo-backend and secondary operations
surface. It belongs to advanced controls, personalization, skills, rules,
preferences, connectors, auto-managed projects, telemetry overview, and links
into dashboard/app settings. It should help power users tune Heiwa without
turning routine use into manual console work.

## The Three Planes

Heiwa is taught to operators as three planes that compose one flow.

**User-facing one-liner:**

> Heiwa watches what matters, summarizes what changed, stages what needs action, executes what is safe, and proves what happened.

| Plane         | What it does                                                                                                                                                | Current repo surfaces                                                                                                                            |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Intake**    | One command bar plus passive feeds. Captures intent and signal from operator commands, mail, calendar, messages, forums, GitHub, files, and runtime alerts. | `apps/heiwa_shell/` REPL and `session attach` are the current intake surface. Passive feeds are target work.                                     |
| **Execution** | DREX routes work to local models, provider CLIs, tools, workers, or connectors under leases, budgets, and approval gates.                                   | `apps/heiwa_core/` (DREX), `apps/heiwa_orchestrator/`, `crates/heiwa_loop/`, `crates/heiwa_provider/`, `crates/heiwa_session/`.                  |
| **Evidence**  | Every useful read or action emits a source-linked receipt visible locally, and mirrored to SpacetimeDB when online.                                         | `crates/heiwa_stdb/`, `apps/heiwa_core/src/stdb/`, `apps/heiwa_orchestrator/src/stdb/`. Receipt schema and source-span syntax are still partial. |

The planes are a flow lens. They sit alongside the layer anatomy in [What Heiwa Is](#what-heiwa-is) (user surface, execution kernel, enterprise platform), which is an ownership lens. Both are correct: planes describe **how a task flows**, layers describe **who owns what**.

### Classification rule

Every feature, connector, doc change, and release item must classify as one of:

- **Intake** — captures intent or external signal
- **Execution** — routes, runs, or stages work
- **Evidence** — records, exposes, or proves what happened
- **Out of scope** — does not advance any plane and should be deferred or rejected

### Current vs target maturity per plane

| Plane     | Current (2026-05)                                                                                                                                                                                                                                                                          | Target                                                                                                             |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| Intake    | `heiwa` REPL plus `session attach`. No passive feeds wired.                                                                                                                                                                                                                                | Command bar plus calendar, mail, messages, forums, GitHub, files, and runtime alerts as governed feeds.            |
| Execution | DREX kernel plus provider adapters: Claude Code, Codex, Gemini CLI, and Ollama are wired in the shell adapter path; Antigravity is discovered and normalized; Codex execution depth and evidence still lag. Bounded loops are real in `crates/heiwa_loop/`. No staged-approval outbox yet. | Approval-staged outbox for every risky write action. Honest per-provider execution depth surfaced at routing time. |
| Evidence  | Local state under `~/.heiwa/` plus partial STDB mirror. Receipt schema not fully canonical. Source-span syntax (`file:line-line`, `message_id`, `event_id`, `thread_id`, `receipt_id`) not implemented.                                                                                    | Source-spanned receipts on every action. Canonical STDB evidence schema. Local-first; mirrors when online.         |

## Optimization Doctrine

Heiwa does not optimize for “most frontier model calls.” It optimizes for `quality + accuracy + efficiency`.

That means:

- local models are the default working tier
- remote providers are escalation surfaces
- provider-native tools are used when they add real leverage
- raw memory and raw traces are kept when they improve future performance
- every harness is expected to evolve as new model capabilities land

Compression:

> Smallest sufficient model, shortest sufficient context, richest sufficient evidence.

Routing and intent classification must not add a provider-token tax to every task. Preflight classification should be deterministic, rule-based, or local-model by default, with any remote escalation guarded by an explicit budget assertion at the call site.

## Working Context vs Harness Memory

Model context windows are not Heiwa’s memory system. They are only the model’s current working context.

Heiwa owns the broader memory and context system:

- durable user-scoped memory
- project-scoped retrieval
- trace and artifact history
- routing-time context selection

Per task, Heiwa should:

1. retrieve the smallest high-value slice from user and project context
2. attach that slice to the chosen agent or provider session
3. let the model spend its native context window on active work rather than repeated setup

Compression:

> Native context window = working memory.
> Heiwa local state = current durable owner memory.
> SpacetimeDB = evidence sync/adjudication plane.
> Harness job = decide what enters working memory, when, and why.

## What Heiwa Is

Heiwa is not just a web app, not just a router, and not just a wrapper around third-party coding agents.

Heiwa is a system with three distinct layers:

1. **User surface**
   - `heiwa` on the machine is the primary product surface.
   - `Heiwa.app` is the real executable client/display shell over the same runtime. Current installs create a HOME-local launcher bundle; the next packaging step replaces that bridge with the native wrapper without changing runtime authority.
   - Web surfaces exist, but they are not the current center of gravity.

2. **Execution kernel**
   - DREX is the routing, policy, and evidence kernel.
   - It spans Rust runtime behavior, local state, and SpacetimeDB reducers/subscriptions where online.

3. **Enterprise platform**
   - Heiwa normalizes access to provider subscriptions, API keys, local models, device capabilities, evidence, routing policy, and later org governance.

The product, service, and feature boundary is defined in [`docs/product-contract.md`](docs/product-contract.md). If a surface does not advance that contract, treat it as support infrastructure, reference material, legacy, or slop until proven otherwise.

## What Heiwa Is Not

- Not a browser-first coding IDE.
- Not a thin BYOK proxy.
- Not a single-provider company wearing a multi-provider skin.
- Not a claim that all current repo surfaces are equally mature.

## Current Repo Truth on `main`

As of 2026-04-22, `heiwa-universe` has already landed meaningful local runtime substrate and narrow terminal productization work:

- The Rust workspace now includes:
  - [`apps/heiwa_core/`](apps/heiwa_core/)
  - [`apps/heiwa_orchestrator/`](apps/heiwa_orchestrator/)
  - [`apps/heiwa_shell/`](apps/heiwa_shell/)
  - [`crates/heiwa_stdb/`](crates/heiwa_stdb/)
  - [`crates/heiwa_session/`](crates/heiwa_session/)
  - [`crates/heiwa_repl/`](crates/heiwa_repl/)
  - [`crates/heiwa_provider/`](crates/heiwa_provider/)
  - [`crates/heiwa_install/`](crates/heiwa_install/)
  - [`crates/heiwa_loop/`](crates/heiwa_loop/)
  - [`packages/heiwa_bindings/rust/`](packages/heiwa_bindings/rust/)
- The current `heiwa` binary already exposes:
  - `install`
  - `doctor`
  - `auth`
  - `providers`
  - `session attach`
    via [`apps/heiwa_shell/src/main.rs`](apps/heiwa_shell/src/main.rs).
- The Rust shell/session/repl/telemetry surface is real enough to test, but it is not the same thing as final product maturity.
- `apps/heiwa_app/` is the companion visual shell path, but it is still a web client surface in this repo and not yet a true native desktop wrapper.
- The Heiwa account/provider plane exists in a narrow but real form, with local identity and wrapped provider status discovery.
- Bounded loop execution is now a real workflow in [`crates/heiwa_loop/`](crates/heiwa_loop/) rather than a stub.
- Python remains in the repo as a compatibility and migration surface. It is not the long-term product center.
- The old Hub module (`heiwa_hub`) was removed from the repo on 2026-07-06; it survives in git history and in the local operator archive. It is not a current product spine or mutation target.
- Provider execution parity is still uneven:
  - Ollama, Claude Code, and Gemini CLI are live shell adapters in the current Rust runtime.
  - Codex is now wired into the same shell adapter path, but broader execution parity and evidence/tool depth still lag.
  - Antigravity is discovered and normalized, but not yet at the same execution depth.
- Online-local backend sync is still less mature than the local shell/runtime path.
- `/code` and broader remote product surfaces remain later work.

## Canonical Product Identity

| Name               | Canonical meaning                                                                           |
| ------------------ | ------------------------------------------------------------------------------------------- |
| **Heiwa**          | Product identity: app, runtime, CLI, packages, docs, and user-visible system                |
| **Heiwa Limited**  | Company, publisher, employer, legal/commercial identity                                     |
| **Heiwa Universe** | Open-source repository and project workspace: `Strategizing/heiwa-universe`                 |
| **`heiwa`**        | Primary installed runtime and operator surface                                              |
| **DREX**           | Internal execution kernel and routing substrate                                             |
| **SpacetimeDB**    | Backend adjudication, canonical state, subscriptions, evidence                              |
| **Rust runtime**   | Volatile execution plane: provider supervision, candidate generation, shell/process control |
| **Web surfaces**   | Later attached or hosted surfaces over the same kernel                                      |

> See [`PRODUCT_SURFACE.md`](PRODUCT_SURFACE.md) for the path-by-path class table that feeds repo hygiene and LOC audits.

Compression:

> Rust proposes, SpacetimeDB adjudicates, `heiwa` presents.

That is the architecture. DREX is not the public brand. It is the kernel inside the Heiwa product.

## Authority Contract

### Rust runtime owns

- Provider subprocess supervision
- Local shell and PTY execution
- Volatile health and liveness observation
- Candidate generation and routing inputs
- Adapter normalization
- Local spool/buffer behavior
- Interacting with external systems and side effects

### SpacetimeDB owns

- Canonical state transitions
- Reducer-enforced mutations
- Adjudicated routing and evidence records
- Session, lease, run, artifact, failure, and routing tables
- Real-time subscriptions
- The durable system-of-record view of execution

### `heiwa` owns

- Operator input surface
- Local install and doctor flows
- Local auth/config UX
- Command invocation and shell escape
- Presentation of runtime and evidence state

### Important user-facing boundary

Normal users and operators should not have to think about SpacetimeDB directly. They interact with Heiwa surfaces; Heiwa services and runtimes own the backend connection path on their behalf.

### Important nuance

SpacetimeDB is where Heiwa keeps and adjudicates truth. Rust is not a dumb pipe, and SpacetimeDB is not a process supervisor. Reducers should remain deterministic and canonical. Rust should continue to own process reality, provider supervision, and external I/O. [1][2][3]

## Topology Modes

### 1. Offline local

- User runs `heiwa` locally.
- Only local tools and local models work.
- No cloud provider inference.
- No OAuth flows.
- No hosted Heiwa sync.
- No web attach.

If there is no internet connection, Claude Code, Codex cloud, Gemini cloud, or other hosted provider execution is unavailable. Full stop.

### 2. Online local

- User runs `heiwa` locally.
- Local runtime is primary.
- Connected cloud providers can be used.
- Heiwa account state, settings, routing preferences, receipts, and history can sync.
- Local models remain first-class and may be preferred for privacy or cost.

### 3. Cloud-backed local

- User still runs `heiwa` locally.
- The local runtime owns the hot path: provider streams, PTY/shell work, local models, device resources, local approvals, and side effects.
- SpacetimeDB Cloud owns durable truth: reducers, subscriptions, session/routing tables, leases, evidence, and audit state.
- Cloudflare owns edge/public surfaces: DNS, docs, app shell, status pages, and later remote attach.
- GitHub owns source, CI, release artifacts, installer distribution, and public repo trust once the secure publish gate passes.

No hosted Rust service tier is required in this topology. If Stage 4+ adds a hosted control plane later, it must not become a hidden inference middleman for the local runtime path.

## Provider, Auth, Model, and Limit Truth

### Separate the two limit systems

There are always two limit planes:

1. **Provider limits**
   - Owned by the connected provider account or local runtime
   - Examples:
     - Claude Code subscription availability
     - OpenAI / Codex quotas
     - Gemini CLI or Google quotas
     - OpenRouter provider availability
     - Local model capacity on a device

2. **Heiwa platform limits**
   - Owned by Heiwa
   - Examples:
     - Plan entitlements
     - Org policy
     - Concurrency caps
     - Hosted session limits
     - Team and enterprise governance features

A task is eligible only if it satisfies both the provider-side constraint set and the Heiwa-side policy set.

### Current provider reality in this repo

- Claude Code, Codex, Gemini CLI, and Antigravity are wrapped as provider-owned CLI surfaces.
- Ollama is the canonical local-runtime provider today.
- Discovery, auth status wrapping, and routing metadata do not imply equal execution depth across all providers.
- Heiwa should always say which providers are merely wrapped, which are verified connected, and which are actually execution-capable for a given workflow.

### Provider auth is per integration mode

Heiwa should normalize provider integrations through explicit auth modes rather than pretending every provider works the same way:

| Auth mode       | Typical use                                                    |
| --------------- | -------------------------------------------------------------- |
| `oauth_cli`     | Installed provider CLIs and subscription-backed terminal tools |
| `oauth_device`  | Browser/device-code or hosted account linking flows            |
| `api_key`       | Direct provider APIs or router providers                       |
| `local_runtime` | Ollama, GGUF runners, future 1-bit / sovereign model runtimes  |

### Model ownership is not Heiwa-owned

Cloud model inventory is usually provider-defined and account-dependent. Heiwa should discover and normalize it; Heiwa does not invent it.

Local model inventory is device-defined. Heiwa should discover it from local runtimes and route accordingly.

### Current product posture

Heiwa should normalize access to:

- Claude Code
- Codex / OpenAI coding surfaces
- Gemini CLI
- Antigravity
- Ollama and later other local runtimes
- API-key providers where direct API access makes sense
- Router providers such as OpenRouter where that improves coverage

But integration maturity is not identical across all of them. Honesty matters more than breadth theater.

## Device Truth

Heiwa is device-aware even when it starts with one machine.

A device is the universal execution noun:

- one MacBook
- one Linux box
- one WSL instance
- one remote runner
- one local Ollama box
- one helper node

Every important execution decision should be expressible against devices:

- local-only vs remote-capable
- writable vs read-only
- local model inventory
- provider CLI availability
- trust tier
- locality
- throughput
- privacy

That is how Heiwa scales from “my machine” to “my fleet” without changing the system model.

## Infrastructure Contract

Heiwa is local-first. Hosted infrastructure exists to provide durable truth, public surfaces, and distribution without moving the inference/shell hot path off the operator device.

| Surface               | Role in Heiwa                                                          |
| --------------------- | ---------------------------------------------------------------------- |
| **GitHub**            | Source of truth, CI, release artifacts, install/update distribution    |
| **SpacetimeDB Cloud** | Canonical state, reducers, subscriptions, evidence                     |
| **Cloudflare**        | Public edge, DNS, docs/app surfaces, later remote access surfaces      |
| **Local machine**     | Primary `heiwa` runtime, provider CLIs, local models, operator control |

| Layer                       | Host                              | Role                                                                     |
| --------------------------- | --------------------------------- | ------------------------------------------------------------------------ |
| Canonical state / evidence  | SpacetimeDB Cloud                 | Reducers, subscriptions, session/routing tables, adjudication            |
| Local inference + streaming | `heiwa` app runtime on the device | Provider streams, PTY/shell, local models, local approvals, side effects |
| Edge / public surfaces      | Cloudflare Workers + Pages        | DNS, docs, web shell, status, later remote attach                        |
| Source / CI / distribution  | GitHub                            | Releases, install artifacts, binaries, checksums, source trust           |

Architectural implication:

- There is no hosted Rust service tier in the v0.1 topology.
- Heiwa.app and the installed `heiwa` runtime run solely on user devices; Heiwa does not provide a hosted app/runtime service.
- Rust runtime is device-local until a later hosted control plane has a verified need.
- SpacetimeDB Cloud is the hosted backend authority, not an operator surface.
- No cloud hop belongs in the inference loop unless the selected provider itself is cloud-hosted.

### SpacetimeDB

SpacetimeDB reducers are the only way to mutate tables. Reducers run transactionally, are deterministic, and cannot do external I/O. Subscriptions replicate rows to clients in real time. Public/private table visibility is explicit. [1][2][3]

Heiwa implication:

- Canonical routing decisions, session state, leases, runs, artifacts, failures, and later policy/state subscriptions belong here.
- Provider subprocess control, shell access, and networked side effects stay in Rust runtime.

### Cloudflare

Cloudflare Workers and Pages are strong fits for Heiwa’s edge and public surfaces:

- Workers Custom Domains are the right model when the Worker is the application origin. [4]
- Pages is appropriate for the public web shell and static/full-stack edge delivery later. [5]

Heiwa implication:

- Cloudflare is an edge and public-surface layer, not the definition of the product.
- The local `heiwa` runtime still matters even if hosted surfaces grow.

## What Makes Heiwa Come Alive

Heiwa is alive when a user can:

1. Sign into Heiwa
2. Connect provider accounts and local model runtimes
3. Launch `heiwa`
4. Route work across those connected resources
5. Keep settings, personalization, evidence, and history coherent across runs

Not when every later platform surface is done.

The minimum living system is:

- account plane
- provider connection plane
- local `heiwa` runtime
- routing and evidence spine
- basic sync of settings/history/personalization

## What We Have Been Doing

The repo has been moving in the right order:

1. tighten the Rust/STDB authority substrate
2. land the local `heiwa` shell/runtime
3. normalize provider accounts and auth status
4. make bounded loop execution real
5. clean up docs so the repo describes Heiwa honestly

That is the right direction.

## What We Should Be Doing Now

Prioritize the remaining product-alive gaps, not maturity theater:

1. strengthen the local MacBook/operator runtime
2. tighten honest provider verification and adapter coverage
3. complete the internal online-local backend sync path
4. keep evidence and bounded execution trustworthy
5. defer `/code`, marketplace, and heavy remote surfaces until the local runtime is undeniably solid

## Development Order

This is the canonical build order from “coming to life” to end-state enterprise power.

### Stage 0: Contracts and hygiene

Lock these before more abstraction churn:

- authority contract
- topology modes
- ownership/secrets/provider-limits contract
- extension classes
- dependency and deployment baseline cleanup

This includes:

- pinned runtime floors
- builder policy clarity
- health path consistency
- binding version alignment
- removal or population of empty stub surfaces

### Stage 1: Heiwa comes alive

Build the minimum real product:

- Heiwa account and identity plane
- provider connect/disconnect/status
- local model discovery
- local `heiwa` runtime
- install / doctor / auth / providers / session basics
- routing and evidence that actually persist useful truth

This is the first meaningful product threshold.

Near-term execution stays inside this threshold:

1. Tighten repo hygiene and doctor correctness.
2. Extend existing `heiwa doctor` checks before adding new command nouns.
3. Add doctrine lint only where it protects existing authority boundaries.
4. Extend `~/.heiwa/config.toml` for local profile and BYOX registration defaults rather than adding another profile file.
5. Persist useful routing and execution evidence through the SpacetimeDB plane.

### Stage 2: Heiwa becomes compelling

Strengthen the local and online-local experience:

- better persistent sessions
- strong provider normalization
- offline local mode that works honestly
- online-local sync that does not feel bolted on
- bounded loop execution
- clear receipts, artifacts, and failure classes

### Stage 3: Heiwa becomes a platform

Expose the kernel progressively:

1. config
2. SDK
3. subscriptions
4. later hooks and reducer-authoring surfaces

This is where Heiwa stops being only a tool and becomes a substrate.

Defer named platform surfaces such as `heiwa task`, `heiwa rules`, `heiwa registry`, and `heiwa optimize` until `heiwa doctor`, bounded local execution, and STDB-backed evidence are trustworthy on one machine.

### Stage 4: Heiwa becomes a team product

Add:

- org policy
- shared routing controls
- device/fleet governance
- hosted assist flows
- team-aware evidence and approvals

### Stage 5: Heiwa becomes an enterprise system

Add:

- hosted control plane maturity
- enterprise auth, audit, and policy layers
- compliance-ready deployment paths
- large-scale fleet and device management
- deeper hosted / local coexistence

### Stage 6: Target end state if Heiwa wins

If Heiwa wins, the company does not win by becoming one more model vendor. It wins by becoming the default AI operating layer above provider fragmentation.

That end state looks like:

- one Heiwa account plane
- one provider/account registry
- one device mesh
- one evidence ledger
- one local runtime
- one hosted control plane
- one policy substrate
- one extension model

The target is not “own all models.”\
The target is “own the layer users trust to use all models, all devices, and all enterprise controls coherently.”

## Peer-Verified Reality Check

Use this when comparing Heiwa to OpenHuman, Hermes, Claude Code, Codex, Gemini
CLI, and similar agent surfaces.

- Hermes should be mined for learning loop, skills, FTS5 recall, Honcho user
  modeling, messaging gateway, cron delivery, MCP, provider/model switching, and
  terminal backends. Do not call it a worker mesh.
- OpenHuman should be mined for desktop onboarding, local Memory Tree,
  Obsidian-style Markdown vault, Composio/OAuth connector breadth, TokenJuice,
  and voice/meeting surface. Do not call it pure local-first; its README states
  default managed services for sign-in, model routing, search proxying, OAuth,
  and Composio-backed integrations.
- Tauri 2 is Heiwa's chosen app foundation because it fits Rust + Solid/Vite +
  local runtime authority. Do not claim OpenHuman proves plain Tauri 2; it uses
  vendored Tauri/CEF sources.
- Heiwa's defensible distinction is provider-peer local ownership: Claude Code,
  Codex, Gemini CLI, Antigravity, Ollama, APIs, local models, machines,
  approvals, receipts, and STDB evidence sync under one operator seat.
- Current gap: Heiwa does not yet match OpenHuman connector breadth or
  TokenJuice, nor Hermes skill self-improvement, gateway delivery, or cron
  breadth. Say "target" until runtime code proves parity.

## Progressive Exposure Model

Heiwa should expose its kernel in this order:

1. **Config**
   - lowest-friction control
   - provider/account defaults
   - routing defaults

2. **SDK**
   - per-request policy control
   - structured client integrations

3. **Subscriptions**
   - real-time routing/evidence visibility
   - live provider/device state observation

4. **Reducers / higher-trust policy extensions**
   - last, not first
   - reserved for power users and enterprise-grade policy surfaces

## Extension Classes Must Stay Separate

Do not collapse everything into “plugins.”

| Class                   | Meaning                                                                      |
| ----------------------- | ---------------------------------------------------------------------------- |
| **Provider adapters**   | Trusted system code that starts/stops providers and normalizes their streams |
| **Tools**               | Callable actions such as shell, MCP, or local services                       |
| **Hooks**               | User-facing block/modify/observe logic over event streams                    |
| **Reducers / policies** | Highest-trust canonical logic in SpacetimeDB                                 |

This separation is necessary for security, determinism, and platform clarity.

## BYOX Boundary

BYOX is a user-facing procurement and registration vocabulary, not an internal trust or execution taxonomy.

Useful product vocabulary:

| Term     | Meaning                                   |
| -------- | ----------------------------------------- |
| **BYOM** | Bring your own model                      |
| **BYOK** | Bring your own key or provider credential |
| **BYOT** | Bring your own tool                       |
| **BYOA** | Bring your own agent or provider account  |
| **BYOD** | Bring your own data source                |
| **BYOP** | Bring your own policy                     |

Internal execution must still branch on the extension classes above: provider adapters, tools, hooks, and reducers or policies. A registered BYOX resource must be mapped into one of those classes before it can affect execution.

Do not let reducers, provider adapters, or routing policy branch directly on broad BYOX labels. BYOX belongs at the registration and operator UX edge; extension classes belong in the runtime, security, and evidence core.

## Security and Secret Boundaries

The following rules are canonical:

- Raw provider secrets should prefer local secure storage or another secure boundary, not casual STDB storage.
- SpacetimeDB may store auth metadata, references, status, expiry, and audit/evidence facts.
- Reducers and hooks should not receive unrestricted raw secrets.
- Local operator/runtime concerns must remain separate from tenant user auth.
- Public web surfaces are not privileged control planes.

See [`docs/security.md`](docs/security.md) for the existing public-surface trust model, but interpret it through the local-runtime-first posture in this file.

## What Should Not Be Built Too Early

Do not mistake maturity theater for leverage.

These are important, but should not outrun substrate truth:

- `/code` as a polished remote coding surface
- a large browser console masquerading as the product
- pretty telemetry for its own sake
- team fleet dashboards before device truth is solid
- WASM reducer marketplaces before config, SDK, and subscriptions exist
- a giant knowledge pipe before bounded loop execution is real

## Reference Systems Worth Mining, Not Copying

These systems are useful reference material for Heiwa, but none of them should be copied literally.

### Junie

Useful for:

- `/account` and `/model` UX
- BYOK plus first-party account coexistence
- terminal-native agent ergonomics

Relevant docs: BYOK, terminal usage, quickstart, CLI/env vars. [10][11][12][13][14]

### Claude Code

Useful for:

- deterministic lifecycle hooks
- block/modify/observe patterns
- strong operator control over agent behavior

Relevant docs: hooks reference. [15]

### OpenRouter

Useful for:

- policy vocabulary
- provider object structure
- fallback, ordering, parameter, and data-collection controls

Relevant docs: provider routing. [16]

### OpenAI Codex

Useful for:

- local/cloud coding-agent framing
- internet-access safety posture for cloud tasks
- multi-surface coding workflow expectations

Relevant docs: code generation and agent internet access. [17][18]

### Ollama

Useful for:

- local model plane
- sovereign execution
- compatibility with coding tools
- a path to make local models feel practical for developers

Relevant docs/blog: `ollama launch`. [19]

### AutoAgent

Useful for:

- bounded keep/discard loops
- evaluation-driven iteration
- anti-overfitting posture

Reference: [kevinrgu/autoagent](https://github.com/kevinrgu/autoagent)

### pi-mono and claw-code

Useful for:

- monorepo package decomposition
- provider/runtime/tool/client separation
- terminal-first agent architecture

References:

- [badlogic/pi-mono](https://github.com/badlogic/pi-mono)
- [ultraworkers/claw-code](https://github.com/ultraworkers/claw-code)

## Canonical Non-Negotiables

- Heiwa-first naming
- local-first truth
- honest maturity statements
- provider-neutral normalization
- evidence-first execution
- device-aware routing
- Rust as primary product implementation
- Python as bounded compatibility surface
- SpacetimeDB as canonical adjudication and evidence sync backend (the Evidence plane is the flow; STDB is the backend that materializes it)
- progressive exposure of internals, not opaque platform behavior

## Companion Context Files

This document is the canonical architecture truth. Two short companion files at `ops/context/` carry navigational and continuity context:

| File                                                 | Purpose                                                   |
| ---------------------------------------------------- | --------------------------------------------------------- |
| [`ops/context/HEIWA.md`](ops/context/HEIWA.md)       | Task routing map and ops/rooms index for agents           |
| [`ops/context/IDENTITY.md`](ops/context/IDENTITY.md) | Operator identity note (referenced by `IDENTITY.md` shim) |
| [`ops/context/SOUL.md`](ops/context/SOUL.md)         | Continuity / persona layer (referenced by `SOUL.md` shim) |

The repo-root files `IDENTITY.md` and `SOUL.md` are compatibility shims that forward to the `ops/context/` counterparts; legacy boot sequences still open them at repo root.

## References

1. [SpacetimeDB reducers overview](https://spacetimedb.com/docs/functions/reducers/)
2. [SpacetimeDB functions overview](https://spacetimedb.com/docs/functions/)
3. [SpacetimeDB subscriptions](https://spacetimedb.com/docs/subscriptions/)
4. [Cloudflare Workers routes and domains](https://developers.cloudflare.com/workers/configuration/routing/)
5. [Cloudflare Pages overview](https://developers.cloudflare.com/pages/)
6. [Junie BYOK](https://junie.jetbrains.com/docs/byok.html)
7. [Junie terminal usage](https://junie.jetbrains.com/docs/junie-cli-usage.html)
8. [Junie quickstart](https://junie.jetbrains.com/docs/junie-cli.html)
9. [Junie CLI reference](https://junie.jetbrains.com/docs/parameters.html)
10. [Junie environment variables](https://junie.jetbrains.com/docs/environment-variables.html)
11. [Claude Code hooks reference](https://code.claude.com/docs/en/hooks)
12. [OpenRouter provider routing](https://openrouter.ai/docs/guides/routing/provider-selection)
13. [OpenAI code generation / Codex overview](https://platform.openai.com/docs/guides/code-generation)
14. [OpenAI Codex agent internet access](https://platform.openai.com/docs/codex/agent-network)
15. [Ollama launch](https://ollama.com/blog/launch)
