# Pi-Mono Adoption Recommendations — 2026-04-25

> **Type:** Report (recommendations). Supersedes the March 23 comparison's general framing with concrete adopt/skip/defer line-items.

**Source repos:**

- [badlogic/pi-mono](https://github.com/badlogic/pi-mono)
- [ultraworkers/claw-code](https://github.com/ultraworkers/claw-code)

**Prior comparison:** `docs/pi_mono_comparison.md` (2026-03-23)

**Codex scope item:** Step 5 — "Adopt pi-mono standards selectively. Borrow provider normalization, session tree thinking, extension UX, and terminal polish. Do not borrow permissive shell safety or TypeScript-as-runtime-control-plane."

---

## Adoption philosophy

Heiwa is Rust-first with SpacetimeDB authority. Pi-mono is TypeScript-first with in-process state. The patterns transfer; the substrate does not. Rule of thumb:

- **Adopt:** UX patterns, abstraction shapes, hygiene defaults, naming conventions
- **Skip:** TypeScript runtime control plane, in-process auth state, permissive shell
- **Defer:** GPU/vLLM deployment patterns (orthogonal to local-first posture)

## Recommendations

### A. Adopt — high confidence, low cost

#### A1. Provider object structure (from pi-mono `@pi/providers`)

**What:** Pi-mono normalizes every provider through a small `Provider` interface with `id`, `displayName`, `models`, `capabilities`, `auth`, and `health` fields. Adapters fill those slots.

**Map to Heiwa:** `crates/heiwa_provider` already has provider trait scaffolding; align its fields to this normalized shape so SDK consumers see one schema across Claude/Codex/Gemini/Antigravity/Ollama.

**Effort:** ~1 day. Renames + a few new struct fields. No semantic change.

**Why:** Removes per-provider special-casing in routing logic; makes `heiwa providers` output uniform.

#### A2. Session tree (from pi-mono `@pi/session`)

**What:** Pi-mono models a session as a tree of turns, each turn carrying tool calls, results, and provenance. A child turn is a branch (e.g., agent retry), not a flat append.

**Map to Heiwa:** `crates/heiwa_session` currently has a flat-ish session model. Add a parent-pointer column to session events so retries, branches, and parallel agent dispatches form a real tree. STDB schema migration required.

**Effort:** ~2-3 days incl. schema migration + STDB reducer updates.

**Why:** Evidence becomes navigable. "Why did this run cost 3x?" can be answered by walking the branch.

#### A3. Differential terminal rendering (from pi-mono `@pi-tui`)

**What:** Pi-mono's TUI engine diffs frames before writing to terminal — feels like a modern app, not a scrolling log.

**Map to Heiwa:** `crates/heiwa_tui` is currently primitive line-based output. Adopt `ratatui` (Rust analogue) and structure frames so steady-state TUI updates don't repaint static regions.

**Effort:** ~1 week. Requires retrofitting a few output paths. Worth it.

**Why:** First impression of `heiwa` is the TUI. Polish here disproportionately moves perception of maturity.

#### A4. Strict `AGENTS.md` posture for Class 3

**What:** Pi-mono's `AGENTS.md` is restrictive — explicit allow-list of tools, explicit auth modes, explicit workspace boundary.

**Map to Heiwa:** `~/heiwa-universe/AGENTS.md` exists; tighten to match pi-mono's restrictiveness for Class 3 Codex/Antigravity sessions specifically. (Claude/Gemini already constrained by their own settings.json/GEMINI.md.)

**Effort:** ~half day, document-only.

**Why:** Class 3 peer model needs durable constraints, not session-by-session reminding.

### B. Adopt with adaptation — medium confidence

#### B1. Extension UX — `/account` and `/model` surfaces (from claw-code)

**What:** Claw-code's `/account` and `/model` slash commands give terminal users a one-shot way to check or switch identity / model without leaving the agent surface.

**Map to Heiwa:** Add `heiwa account` and `heiwa model` subcommands to `apps/heiwa_shell/src/main.rs` mirroring the verbs already present (`auth`, `providers`). `account` shows linked Heiwa identity + provider accounts; `model` lists available routes and current default.

**Effort:** ~1 week. Touches CLI parsing, provider crate, identity crate, and TUI output.

**Why:** Maps to the user mental model from competing tools. Reduces "how do I check what I'm signed into" friction. Junie patterns reinforce same shape (already cited in `HEIWA.md`).

#### B2. Bounded keep/discard loop (from AutoAgent)

**What:** Loop runs N candidates, evaluates against a scoring rubric, keeps best, discards rest. Anti-overfitting because each iteration's evidence is bounded.

**Map to Heiwa:** `crates/heiwa_loop` is real but bounded mostly by step count. Add a candidate-set primitive: "run K candidates, score each, keep best." Evidence written to STDB per candidate, including discarded ones, so traces are honest about exploration.

**Effort:** ~1 week. New API + tests + STDB schema for candidate sets.

**Why:** Local-model + remote-model comparison becomes routine. Honest evidence for "we tried 5 things" instead of post-hoc rationalization.

### C. Skip — explicitly do not adopt

#### C1. TypeScript as runtime control plane

**Why skip:** `HEIWA.md` non-negotiable: "Rust as primary product implementation. Python as bounded compatibility surface." TypeScript runtime control would create a third primary language. Web/UI surfaces stay in TypeScript; runtime stays Rust.

#### C2. Permissive shell safety defaults

**Why skip:** Pi-mono's shell adapter accepts most commands by default. Heiwa's posture (per `CLAUDE.md` hard rules: untrusted code → E2B sandboxes only) is the opposite. Approval policy is owned by `docs/approval-and-orchestration-policy.md`.

#### C3. In-process auth state

**Why skip:** Pi-mono holds provider auth in-process for session lifetime. Heiwa's two-layer auth model (per `heiwa_auth_model.md` memory) keeps provider OAuth on-device via OS keychain integration. Different security posture.

#### C4. Single-binary deployment as default

**Why skip:** Pi-mono ships a single TS bundle. Heiwa's local-first runtime depends on having `heiwa` per-device with local STDB connectivity options. Single-binary distribution still happens, but architecture stays multi-component.

### D. Defer — interesting, not now

#### D1. vLLM GPU pod patterns

**Why defer:** Local-first MacBook posture takes precedence. Revisit when first hosted Heiwa user with self-hosted GPU appears.

#### D2. Pi-mono web component library

**Why defer:** `apps/heiwa_app` is web-client form, but the visual shell is not the center of gravity (per `HEIWA.md`). Revisit when web surface gets actual product investment.

#### D3. Pi-mono's plugin marketplace shape

**Why defer:** Per `HEIWA.md`: "WASM reducer marketplaces before config, SDK, and subscriptions exist" is anti-pattern. Marketplace comes after the substrate is undeniable on one machine.

## Suggested execution sequence

If picking off one per ~week opportunistically:

1. **A4** (strict AGENTS.md) — half day, no risk, immediate hygiene win
2. **A1** (provider object structure) — locks shape before more adapters land
3. **A3** (TUI diff rendering) — perception win, mostly mechanical
4. **A2** (session tree) — schema migration, plan as its own dedicated PR
5. **B1** (`/account`, `/model` surfaces) — user-facing, do after A1+A2 settle
6. **B2** (candidate-set loop) — most ambitious, last

## What this report does NOT do

- Mandate any of these be done (recommendations only)
- Replace existing crate-level plans (each adoption gets its own ticket if executed)
- Comment on whether to depend on pi-mono packages directly (we extract patterns, not deps)
- Re-litigate the March 23 comparison (read it for general background; this doc is action-only)
