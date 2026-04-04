# HEIWA

Updated: 2026-04-04  
Status: Canonical truth for `heiwa-universe`

This file replaces the old repo-root compatibility shim. When `README.md`, legacy plans, or older architecture notes conflict with this document, this document wins.

## One-Sentence Truth

Heiwa is a local-first AI runtime and enterprise platform: `heiwa` is the installed product surface, DREX is the internal execution kernel, SpacetimeDB is the adjudication and evidence plane, Rust proposes and executes, and `heiwa` presents.

## What Heiwa Is

Heiwa is not just a web app, not just a router, and not just a wrapper around third-party coding agents.

Heiwa is a system with three distinct layers:

1. **User surface**
   - `heiwa` on the machine is the primary product surface.
   - Web surfaces exist, but they are not the current center of gravity.

2. **Execution kernel**
   - DREX is the routing, policy, and evidence kernel.
   - It spans Rust runtime behavior and SpacetimeDB reducers/subscriptions.

3. **Enterprise platform**
   - Heiwa normalizes access to provider subscriptions, API keys, local models, device capabilities, evidence, routing policy, and later org governance.

## What Heiwa Is Not

- Not a browser-first coding IDE.
- Not a thin BYOK proxy.
- Not a single-provider company wearing a multi-provider skin.
- Not a claim that all current repo surfaces are equally mature.

## Current Repo Truth on `main`

As of 2026-04-04, `heiwa-universe` has already landed meaningful local runtime substrate and narrow terminal productization work:

- The Rust workspace now includes:
  - [`apps/heiwa_core/`](apps/heiwa_core/)
  - [`apps/heiwa_hub/spacetimedb/`](apps/heiwa_hub/spacetimedb/)
  - [`apps/heiwa_shell/`](apps/heiwa_shell/)
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
- Python remains in the repo as a compatibility and migration surface. It is not the long-term product center.
- `crates/heiwa_loop/` exists, but bounded loop execution is not yet the canonical finished workflow.
- `/code` and broader remote product surfaces remain later work.

## Canonical Product Identity

| Layer | Canonical meaning |
| --- | --- |
| **Heiwa** | Company and product identity |
| **`heiwa`** | Primary installed runtime and operator surface |
| **DREX** | Internal execution kernel and routing substrate |
| **SpacetimeDB** | Adjudication, canonical state, subscriptions, evidence |
| **Rust runtime** | Volatile execution plane: provider supervision, candidate generation, shell/process control |
| **Web surfaces** | Later attached or hosted surfaces over the same kernel |

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

### 3. Hosted

- Hosted Heiwa services run on Railway and related infra.
- Web and other clients can attach to hosted sessions or hosted control surfaces.
- Same kernel model, different deployment topology.
- This is not the first thing that must be perfect for Heiwa to come alive.

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

### Provider auth is per integration mode

Heiwa should normalize provider integrations through explicit auth modes rather than pretending every provider works the same way:

| Auth mode | Typical use |
| --- | --- |
| `oauth_cli` | Installed provider CLIs and subscription-backed terminal tools |
| `oauth_device` | Browser/device-code or hosted account linking flows |
| `api_key` | Direct provider APIs or router providers |
| `local_runtime` | Ollama, GGUF runners, future 1-bit / sovereign model runtimes |

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

Heiwa is local-first, but hostable on the chosen infra.

| Surface | Role in Heiwa |
| --- | --- |
| **GitHub** | Source of truth, CI, release artifacts, install/update distribution |
| **Railway** | Hosted Rust services and private service networking |
| **SpacetimeDB Cloud** | Canonical state, reducers, subscriptions, evidence |
| **Cloudflare** | Public edge, DNS, docs/app surfaces, later remote access surfaces |
| **Local machine** | Primary `heiwa` runtime, provider CLIs, local models, operator control |

### Railway

As of current Railway docs:

- Railpack is the default builder.
- New services default to Railpack.
- Nixpacks is deprecated and in maintenance mode.
- Dockerfiles are fully supported, and Railway will use them when present.
- Config-as-code supports explicit `RAILPACK` and `DOCKERFILE` builders. [4][5][6][7]

Heiwa implication:

- Prefer **Dockerfile-first** deployment for critical hosted Rust services.
- Use Railpack only where zero-config speed is worth the trade.
- Do not architect new production surfaces around Nixpacks.

### SpacetimeDB

SpacetimeDB reducers are the only way to mutate tables. Reducers run transactionally, are deterministic, and cannot do external I/O. Subscriptions replicate rows to clients in real time. Public/private table visibility is explicit. [1][2][3]

Heiwa implication:

- Canonical routing decisions, session state, leases, runs, artifacts, failures, and later policy/state subscriptions belong here.
- Provider subprocess control, shell access, and networked side effects stay in Rust runtime.

### Cloudflare

Cloudflare Workers and Pages are strong fits for Heiwa’s edge and public surfaces:

- Workers Custom Domains are the right model when the Worker is the application origin. [8]
- Pages is appropriate for the public web shell and static/full-stack edge delivery later. [9]

Heiwa implication:

- Cloudflare is an edge and public-surface layer, not the definition of the product.
- The local `heiwa` runtime still matters even if hosted surfaces grow.

## Current Repo Drift That Must Be Named Honestly

This repo contains real progress and real drift.

### Canonical current deployment truth

- Root [`railway.toml`](railway.toml) currently uses:
  - `builder = "DOCKERFILE"`
  - `dockerfilePath = "apps/heiwa_core/Dockerfile"`
  - `healthcheckPath = "/health"`
- The main deploy workflow checks:
  - `https://api.heiwa.ltd/health`
  - `https://api.heiwa.ltd/status`
  in [`.github/workflows/deploy.yml`](.github/workflows/deploy.yml).

### Drift that still exists

- [`docs/standards/runtime-baseline.md`](docs/standards/runtime-baseline.md) still says Railway must use `/ready`.
- [`infra/cloud/railway/README.md`](infra/cloud/railway/README.md) still says healthcheck `/ready`.
- The root [`README.md`](README.md) is still more web/dashboard/BYOK-first than the current local-runtime-first direction.

So the canonical truth as of 2026-04-04 is:

> The live repo deployment surface points at `/health`, not `/ready`, and the root docs need harmonization.

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

The target is not “own all models.”  
The target is “own the layer users trust to use all models, all devices, and all enterprise controls coherently.”

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

| Class | Meaning |
| --- | --- |
| **Provider adapters** | Trusted system code that starts/stops providers and normalizes their streams |
| **Tools** | Callable actions such as shell, MCP, or local services |
| **Hooks** | User-facing block/modify/observe logic over event streams |
| **Reducers / policies** | Highest-trust canonical logic in SpacetimeDB |

This separation is necessary for security, determinism, and platform clarity.

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
- a large web console
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
- SpacetimeDB as canonical adjudication and evidence plane
- progressive exposure of internals, not opaque platform behavior

## References

1. [SpacetimeDB reducers overview](https://spacetimedb.com/docs/functions/reducers/)
2. [SpacetimeDB functions overview](https://spacetimedb.com/docs/functions/)
3. [SpacetimeDB subscriptions](https://spacetimedb.com/docs/subscriptions/)
4. [Railway build configuration](https://docs.railway.com/builds/build-configuration)
5. [Railway builds overview](https://docs.railway.com/builds)
6. [Railway config as code](https://docs.railway.com/reference/config-as-code)
7. [Railway Railpack docs](https://docs.railway.com/builds/railpack)
8. [Cloudflare Workers routes and domains](https://developers.cloudflare.com/workers/configuration/routing/)
9. [Cloudflare Pages overview](https://developers.cloudflare.com/pages/)
10. [Junie BYOK](https://junie.jetbrains.com/docs/byok.html)
11. [Junie terminal usage](https://junie.jetbrains.com/docs/junie-cli-usage.html)
12. [Junie quickstart](https://junie.jetbrains.com/docs/junie-cli.html)
13. [Junie CLI reference](https://junie.jetbrains.com/docs/parameters.html)
14. [Junie environment variables](https://junie.jetbrains.com/docs/environment-variables.html)
15. [Claude Code hooks reference](https://code.claude.com/docs/en/hooks)
16. [OpenRouter provider routing](https://openrouter.ai/docs/guides/routing/provider-selection)
17. [OpenAI code generation / Codex overview](https://platform.openai.com/docs/guides/code-generation)
18. [OpenAI Codex agent internet access](https://platform.openai.com/docs/codex/agent-network)
19. [Ollama launch](https://ollama.com/blog/launch)
