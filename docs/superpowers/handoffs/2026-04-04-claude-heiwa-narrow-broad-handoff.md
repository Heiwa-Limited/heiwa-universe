# Claude Handoff — Heiwa Narrow-to-Broad Context

## Narrow: Immediate Repo Truth

- Repo: `heiwa-universe`
- Path: `/Users/dmcgregsauce/heiwa-universe`
- Branch: `main`
- Current HEAD: `65fee5a`
- Remote: `origin/main` matches local `main`
- Worktrees: only the root repo worktree remains
- Current uncommitted state when this handoff was written:
  - `docs/superpowers/plans/2026-04-04-heiwa-local-runtime-piped-implementation.md` untracked
  - this handoff file added alongside it

### What has already landed on `main`

Recent verified commits:

- `65fee5a` `feat: add device-aware DREX capability routing and strength bonuses`
- `c757496` `feat: add local provider adapter layer and ollama scaffold`
- `a9c24d0` `feat: add canonical provider account and auth plane`
- `e60e427` `feat: add heiwa install and doctor flows`
- `b07de09` `feat: scaffold rust heiwa runtime workspace and fix stdb syntax error`
- `a0e0879` `feat: first-class run failures and replay evidence tightening`

### Immediate instruction to Claude

Do **not** assume Heiwa still needs the early Phase A scaffold work. That substrate has already started landing on `main`.

The immediate job is:

1. verify the actual completeness and quality of the landed Phase A slices
2. identify the remaining gaps between the committed code and the intended substrate
3. avoid starting terminal polish or `/code` work unless the substrate is genuinely ready

### Active planning artifact

Primary current plan:

- `docs/superpowers/plans/2026-04-04-heiwa-local-runtime-piped-implementation.md`

This plan was deliberately tightened to enforce a foundation-first boundary:

- **Phase A** only
- terminal UX later
- loops later
- web `/code` later

Claude should treat that plan as the current execution contract unless Devon explicitly changes it.

---

## Mid: Platform Truth

### Core architecture

Heiwa is converging on this split:

- **Rust** = authority spine
- **STDB** = canonical truth
- **Python** = bounded compatibility and migration surface
- **TypeScript** = tooling and later UI surface, not primary runtime authority
- **Railway** = managed control/runtime plane
- **Cloudflare** = public edge and later remote/app surface
- **GitHub** = source-of-truth code transport and CI boundary

### Product center

The primary product is **not** the web app.

The primary product is the installed local runtime:

- `heiwa`
- machine-installed
- local-first
- provider-aware
- evidence-first
- later remotely attachable

For now, `/code` is explicitly deferred. Do not let web ambitions distort local runtime sequencing.

### Canonical runtime principles

- `heiwa` is the product surface
- `!` means direct shell passthrough inside the active session
- `/command` means Heiwa-native command handling
- local provider CLIs and local models are first-class execution surfaces
- DREX chooses provider/device/model/runtime
- every action should emit receipts, artifacts, and failures
- Python must stop being the long-term CLI authority

### Current repo surfaces that matter most

Rust authority:

- `apps/heiwa_core/src/runtime/gateway.rs`
- `apps/heiwa_core/src/runtime/state.rs`
- `apps/heiwa_core/src/stdb/mod.rs`
- `apps/heiwa_core/src/drex/mod.rs`
- `apps/heiwa_core/src/drex/router.rs`
- `apps/heiwa_core/src/drex/scorer.rs`
- `apps/heiwa_core/src/auth.rs`
- `apps/heiwa_hub/spacetimedb/src/lib.rs`

Rust product workspace started:

- `apps/heiwa_shell/`
- `crates/heiwa_install/`
- `crates/heiwa_provider/`
- `crates/heiwa_session/`
- `crates/heiwa_repl/`
- `crates/heiwa_loop/`

Python compatibility surface:

- `packages/heiwa_cli/heiwa_cli/__main__.py`
- `packages/heiwa_cli/heiwa_cli/repl.py`
- `packages/heiwa_cli/heiwa_cli/commands.py`
- `packages/heiwa_cli/heiwa_cli/status_line.py`
- `packages/heiwa_cli/heiwa_cli/auth.py`
- `packages/heiwa_cli/heiwa_cli/context.py`
- `apps/heiwa_cli/bin/heiwa`

TypeScript/tooling baseline:

- `package.json`
- `tsconfig.base.json`
- `apps/heiwa_web/package.json`
- `apps/heiwa_web/tsconfig.json`
- `packages/heiwa_bindings/typescript/`

### Infra truth

- **GitHub**
  - mainline development source
  - current repo state is on `main`
  - use it for code transport and CI, not as a substitute for product state

- **Railway**
  - control-plane and deploy runtime
  - not the product surface itself
  - useful for auth/control services and shared runtime pieces

- **SpacetimeDB**
  - canonical truth for sessions, provider accounts, leases, runs, artifacts, failures, routing decisions
  - must remain the source of truth rather than in-memory or Python-era side state

- **Cloudflare**
  - later public edge/app surface
  - useful for remote attach, edge auth, and public routes
  - not the current build priority

---

## Broad: Company and Product Direction

### Company-level framing

Heiwa is not “another AI chat app.”

Heiwa is a **sovereign hybrid AI execution platform**:

- local-first
- machine-installed
- provider-neutral
- device-aware
- evidence-first
- later remotely attachable

### What Heiwa is selling

Heiwa should eventually offer:

- one local runtime that can orchestrate multiple AI harnesses
- one control plane over installed provider CLIs, local models, and later BYOK/API providers
- one evidence layer for tasks, tools, runs, artifacts, and failures
- one device-aware routing model
- one persistent session model that can scale from single-machine to trusted mesh

### Product surfaces

Near-term product:

- `heiwa install`
- `heiwa doctor`
- `heiwa auth`
- `heiwa providers`
- `heiwa` local runtime

Later product:

- `/code` remote attach surface
- team/org/fleet policy
- billing and account controls
- boost-node and remote broker flows

### Moat

Heiwa’s moat is not model quality.

It is:

- provider normalization
- local sovereignty
- device-aware routing
- strong receipts and failures
- one session model across heterogeneous AI harnesses

---

## External-System Value To Extract

Claude should extract patterns, not copy products.

### `pi-mono`

High-value reference for:

- monorepo package decomposition
- provider abstraction
- model/account registry
- terminal-first agent UX
- hook/extension patterns

Use it to improve Heiwa’s crate/package boundaries and provider registry shape, not to import TS complexity blindly.

### Claude Code hooks

High-value reference for:

- lifecycle events
- tool interception
- normalized event surfaces

Heiwa should mirror the **concept**:
- session start
- pre/post tool use
- approval boundaries
- session end

but implement it as a provider-neutral event bus.

### OpenRouter

Useful for:

- routing-policy vocabulary
- fallback behavior
- provider ordering
- privacy-ish routing knobs

Do not let OpenRouter become Heiwa’s authority model. Treat it as one provider/router kind among many.

### Junie BYOK

Useful for:

- account/provider UX framing
- custom provider support
- multi-provider model selection

Heiwa should adapt that into a local-runtime provider-account plane.

### AutoAgent

Useful for:

- bounded self-improvement loops
- eval/keep/discard discipline
- iteration receipts

Translate into Heiwa `loop`, but only after the substrate is stable.

### claw-code

Useful for:

- Rust crate decomposition
- runtime/commands/tools/plugins/server separation

Take structural lessons, not product assumptions.

---

## What Claude Should Not Do

- Do not start building `/code`
- Do not overbuild the TUI/REPL before substrate acceptance
- Do not let Python remain the hidden authority layer
- Do not make OpenRouter or any single external router central to Heiwa
- Do not copy another system’s TS surface complexity as a first move
- Do not claim “Heiwa is operable” just because a shell binary starts

---

## Recommended Next Moves For Claude

### 1. Audit Phase A against reality

Check whether the landed commits actually produce:

- a coherent Rust workspace
- useful install/doctor flows
- canonical provider accounts in STDB
- a working provider adapter contract
- real device-aware DREX routing

If gaps exist, document them before writing more product-facing code.

### 2. Turn the current plan into an acceptance checklist

Use:

- `docs/superpowers/plans/2026-04-04-heiwa-local-runtime-piped-implementation.md`

Convert Phase A into a pass/fail checklist against real code and tests.

### 3. Verify the local product spine, not the polished UX

Ask:

- Can `heiwa install` reason about runtimes and providers correctly?
- Can `heiwa auth` normalize provider accounts without Python owning truth?
- Can adapters emit normalized events and evidence?
- Can DREX pick between provider/device capabilities?
- Are receipts and failures attached to provider actions?

### 4. Only after that, scope Phase B

If Phase A is genuinely complete, then and only then start:

- PTY/session daemon refinement
- REPL polish
- telemetry
- Python cutover

### 5. Keep broad company direction in view

Claude should keep every local change aligned to this company truth:

Heiwa is building a machine-installed, provider-neutral, evidence-first AI runtime that later expands into remote attach, team control, and trusted multi-device execution.

---

## One-Sentence Compression

Heiwa should feel like one local runtime that can unify Claude Code, Codex, Gemini CLI, Antigravity, Ollama, and later BYOK/router providers under one authoritative session, routing, and evidence model, while deferring web surfaces until that substrate is unquestionably real.

