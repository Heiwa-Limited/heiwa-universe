# Heiwa App Product Roadmap Design

Date: 2026-08-14
Status: Draft for review
Plane: Intake + Execution + Evidence

## Summary

Heiwa becomes a single installable desktop application that consolidates a
user's digital surfaces and acts on them. The Tauri shell is the product, not a
viewer onto a developer's runtime. Every capability ships inside it.

The application is written for N users. Any person can install it, connect
their own accounts, and use it for their own purposes without reading this
repository, without owning this machine, and without having any provider CLI
installed. The single-operator assumption that runs through the current tree is
retired.

GitHub is a user connector like any other: it connects a user's repositories to
their app. Heiwa Limited separately uses GitHub to develop and ship Heiwa. Those
two facts are unrelated, and the second must never leak into the product's
architecture.

This document defines the layer sequence L0 through L5, the interfaces between
layers, and implementation-level detail for L0 and L1 — the two layers a fresh
install cannot function without. L2 through L5 are scoped here with their seams
named; each gets its own spec when reached.

## Problem

The current desktop shell is a ten-surface application whose surfaces are mostly
honest placeholders. `apps/heiwa_app/desktop/src/main.ts` renders Home, AI,
Windows, Calendar, Mail, Finance, Social, Workers, Browser, and Files. Of these,
AI (operator stream), Windows (`herd.rs` panes), and Workers (operator turns)
are backed by real runtime capability. Calendar, Mail, Finance, and Social
render placeholder text: "no loaded events", "draft/write gated", "read model
pending", "ingress pending". Browser is an iframe.

Three structural facts block the product:

**A fresh install does nothing.** `crates/heiwa_provider/src/providers/` holds
five adapters. `claude_code.rs`, `codex_cli.rs`, and `gemini_cli.rs` invoke
`Command::new` against locally installed provider CLIs. `ollama.rs` requires a
local Ollama. Only `openrouter.rs` speaks to an API directly. A user who
installs Heiwa and has none of these gets an application with no inference.

**Four surfaces share one missing dependency.** `connectors/` contains exactly
one manifest, `github.connector.json`. `docs/current-capability.md` states that
"executable connector capability truth beyond manifest validation" is not
complete: manifests validate, nothing executes them. Calendar, Mail, Finance,
and Social are not four features. They are one connector plane wearing four
hats.

**The UI layer cannot carry the target quality.** The desktop frontend has no UI
framework. `main.ts` is 1,322 lines of hand-rolled DOM and `styles.css` is 1,030
lines. Adding four live surfaces, a browser, and an onboarding flow to that
layer produces work that must be rewritten.

## Goals

- A stranger installs Heiwa, supplies one credential, and has a working
  assistant within the first session.
- Every capability is reachable inside the Tauri shell. No capability requires
  a terminal, this repository, or a provider CLI.
- A user's digital surfaces — repositories, mail, calendar, finance, social,
  files, and the web — are connected through one consistent, approval-gated
  execution model with receipts.
- Quality matches or exceeds the ChatGPT and Claude desktop applications, then
  exceeds them in breadth of connected surface and honesty of evidence.
- Provider choice stays the user's. API keys, OAuth, and local runtimes are all
  first-class; no provider is privileged by the architecture.

## Non-Goals

- A hosted inference middleman. Local execution stays on the local machine.
- Reproducing this repository's developer workflow inside the product.
- Cross-device sync in L0 through L4. The seams are placed; the machinery is
  L5 under the transport decision ratified on 2026-08-20.
- Mobile clients. `apps/heiwa_app/clients/iphone` stays out of scope here.

## Product thesis

Heiwa consolidates a user's digital surfaces into one local application and
maximizes what the user can get done with the resources they already have —
their accounts, their machine, their model subscriptions, their time.

The differentiator is not the chat window. It is that every surface is
connected through the same execution kernel, every action is classified by
risk, gated by approval, and recorded as a receipt the user can read back.
Competing desktop applications connect a few surfaces with opaque actions.
Heiwa connects many with auditable ones.

## Constraint change: the N-user presumption

The tree currently assumes one operator on one machine. `CLAUDE.md` states
"Devon's MacBook (M4 Pro, 24GB) is the owner/operator seat".
`ops/context/HEIWA.md` states "Develop for the operator's local machine first."
Under an N-user presumption these become false, and the assumption is likely
load-bearing beyond documentation.

The following must hold for every layer below:

- No hardcoded operator identity, machine name, or home path outside a
  resolved per-user configuration root.
- No capability that assumes a tool the user did not install.
- No first-run step that requires reading documentation in this repository.
- Every error a new user can hit must be actionable inside the application.

An audit of how deeply the single-seat assumption is baked into config and path
handling is a prerequisite task inside L0, not a separate project.

## Layer map

| Layer | Scope | Unlocks | Depends on |
|---|---|---|---|
| L0 | UI foundation and N-user config root | Everything renders here | — |
| L1 | BYOK provider tier | A fresh install works | L0 config root |
| L2 | Onboarding and per-user identity | First-run without docs | L1 |
| L3 | Connector plane | Calendar, Mail, Finance, Social, GitHub | L2 identity, `heiwa_vault` |
| L4 | Browser surface | Web as an actionable surface | L0, DREX gate |
| L5 | Cross-device state | Continuity across machines | L2 identity, ratified D1 |

Heiwa Limited infrastructure — the release chain in `docs/backlog.md` (B6a
through B15), distribution, and installer integrity — runs as a parallel track.
It ships the product; it is not part of the product.

## L0 — UI foundation

### Purpose

Provide a component layer capable of carrying ten live surfaces, and establish
the per-user configuration root that makes the application installable by
anyone.

### Framework

Adopt SolidJS. The repository already uses it in
`apps/heiwa_app/clients/cockpit` (`solid-js 1.9.13` in the root lock, with
`@solidjs/router`), so it introduces no new dependency class, no new build
toolchain, and no second idiom for contributors. Its fine-grained reactivity
suits a shell driven by a high-frequency event stream, which the operator
stream is.

### Decomposition

`main.ts` is split along surface boundaries. Each of the ten surfaces becomes a
component module owning its own render and local state, consuming the operator
store through a typed interface rather than reaching into globals. The existing
`src/operator/store.ts`, `client.ts`, and `types.ts` are retained — they already
have test coverage in `store.test.ts` and `client.test.ts` and they are the
correct seam. The rail, dock, and shell chrome become their own module.

The target is that no surface module requires reading another surface module to
be understood, and that a surface can be replaced without touching the shell.

### Design system

A token layer — color, type scale, spacing, motion, elevation — replaces the
1,030-line stylesheet, with light and dark defined as token sets rather than
overrides. Surfaces consume tokens only. This is what makes "frontier quality"
a property of the system rather than of individual screens.

### Configuration root

A single resolver owns the per-user state root, replacing every direct
reference to a home path. It resolves the platform-correct location, creates it
on first run, and is the only code permitted to know where user state lives.
The single-seat audit lands here: every hardcoded path, operator name, or
machine assumption found in the tree is either routed through this resolver or
removed.

### Interfaces out of L0

- `SurfaceModule` — the contract every surface implements to mount into the
  shell.
- `ConfigRoot` — resolved per-user state location, consumed by all layers.
- Token set — consumed by all surfaces.

## L1 — BYOK provider tier

### Purpose

A user supplies one credential of their choosing and the application works.

### What already exists

`crates/heiwa_provider` is further along than the surface suggests. The registry
exposes `add_api_key_account`, `add_local_runtime_account`, and
`add_cli_account`, so the three-way account model is built. `oauth.rs` provides
`ProviderVault` with keychain-backed storage and `needs_refresh` token
lifecycle. `keychain.rs` handles secret storage. `ProviderAdapter` is a clean
trait: `send`, `interrupt`, `supported_models`.

The gap is adapter coverage, not architecture.

### What is built

Direct-API adapters implementing `ProviderAdapter`, one per major provider,
alongside the existing CLI adapters rather than replacing them. A user with a
provider CLI installed keeps using it; a user without one uses their key. The
CLI adapters stay because they are genuinely better where present — they carry
the provider's own auth, quota, and session behavior, which the provider owns.

Each adapter is responsible only for transport and translation to the shared
`StreamEvent` vocabulary. Routing, quota, and cost remain with DREX and
`heiwa_quota`. No adapter may hardcode a model list that the provider is
authoritative for; model inventory is discovered, and discovery is never
presented as execution support.

### Account model

An account is a credential plus a provider plus a kind. Users may hold several
accounts for one provider and several providers at once — this is the
"cross-account" requirement at its foundation. Selection among a user's
available accounts is a DREX routing decision constrained by that user's
quota, cost, and privacy preferences, not a global default.

### Failure semantics

A provider that is unauthenticated, rate-limited, or unreachable is a routing
constraint, not a crash. The user sees which of their accounts are healthy and
why one was skipped. An application with zero healthy accounts must still open
and must guide the user to connect one.

### Interfaces out of L1

- `ProviderAdapter` implementations for direct-API providers.
- Account health projection, consumed by onboarding (L2) and the AI surface.

## L2 through L5 — scope and seams

### L2 — Onboarding and per-user identity

First run establishes a local user identity, a configuration root, and at least
one provider account, entirely inside the application. Identity is local and
per-installation; it is the anchor that L3 attaches credentials to and L5 would
synchronize. `packages/heiwa_identity` is the starting point. Open question
carried into its spec: whether identity is purely local or optionally backed by
an account, which is the same fork as L5.

### L3 — Connector plane

Turns `connectors/*.connector.json` from validated manifests into executable
capability. Requires a per-user OAuth flow, credential storage through
`heiwa_vault`, a capability vocabulary mapped onto `RiskTier`, and a read model
per surface. `heiwa_automations` already provides executor, scheduler, and
storage and is the execution substrate.

**Superseded 2026-08-15 (AD-14):** this section originally made GitHub the
first connector because its manifest already exists. That optimized for the
easiest credential rather than the stated user. **Calendar and Mail come
first** — an executive assistant for a non-technical person is Calendar and
Mail; GitHub serves developers, and it, Finance, and Social follow on the
same plane. The cost — Google/Microsoft OAuth is the hardest credential work
in L3 — is accepted deliberately.

### L4 — Browser surface

Designed in detail during this session and summarized here.

A new crate `crates/heiwa_browser`, owned by the runtime rather than the shell,
following the `proxy.rs` precedent that the shell proxies to the runtime and
holds no capability logic. The runtime spawns a Chromium sidecar with a
dedicated user-data directory under the configuration root and drives it over
the DevTools Protocol.

Rendering uses CDP `Page.startScreencast` into a canvas in the shell, with input
forwarded via `Input.dispatchMouseEvent` and `dispatchKeyEvent`. This keeps the
surface inside the application window, behaves identically across platforms,
and avoids embedding a native browser view. Media-heavy pages degrade; this is
a browsing and automation surface, not a media player.

The user and the agent share one profile, so the agent inherits the user's real
logins, but tab ownership is enforced: a registry maps each CDP target to
`Owner::User` or `Owner::Agent { session_id }`, and agent-issued commands
against user-owned targets are rejected at the boundary unless the user hands
the tab over explicitly.

Actions classify onto the existing `RiskTier` vocabulary in `crates/heiwa_a2a`
and pass through the `heiwa_drex` gate, which already returns
`ApprovalVerdict::AwaitingApproval { request_id, request_path }`:

| Action | Tier | Gate |
|---|---|---|
| read DOM, screenshot, extract | T0 | automatic |
| navigate, scroll, open tab | T1 | automatic, logged |
| click, fill, submit | T2 | approval required |
| payment, credential entry, destructive | T3 | explicit broker; refused by default |

Page content is treated as untrusted. Instructions found in a page are data,
never commands. The `ProxyError::ProtectedMaterial` guard already present in
`proxy.rs` is the model for refusing to pass page content containing protected
authentication material.

Proposed modules: `sidecar.rs` process lifecycle and health, `cdp.rs` protocol
client, `profile.rs` cookie jar and redaction, `tabs.rs` ownership registry,
`actions.rs` typed vocabulary, `policy.rs` tier mapping into DREX.

### L5 — Cross-device state

Networked L5 remains sequenced after L3/L4. Its local machine-perspective
bootstrap may land earlier because it also closes the existing app boot
contract on one machine.

**Superseded 2026-08-20.** L5 is specified in
`docs/superpowers/specs/2026-08-20-heiwa-mesh-runtime-design.md`, which widens
the layer from "cross-device state" to one governed runtime across the user's
machines. Devon ratified D1 on 2026-08-20: direct device-to-device transport
plus optional user-supplied ciphertext relay, with no hosted authority plane.
L3 and L4 remain unaffected except for the four constraints that spec lists.

## Open decisions

### D1 — Cross-device sync transport

**Resolved by Devon 2026-08-20** as specified in
`docs/superpowers/specs/2026-08-20-heiwa-mesh-runtime-design.md`: candidate 3
(direct device-to-device) as the transport, candidate 1 (user-supplied storage)
as an optional ciphertext relay for the both-offline case. Candidate 2 is
refused only because adopting a hosted authority plane is a product-policy
change that is Devon's to make, not because it lacks merit.

`HEIWA.md` currently records that "redaction-gated evidence sync through GitHub
is the planned (not yet built) multi-device path", and that "no hosted backend
authority plane exists in this topology". The product direction in this document
reassigns GitHub to a user connector, which removes it as the sync transport and
leaves the requirement — persistent state across a user's devices and accounts —
without an answer.

Candidates considered:

1. **User-supplied storage.** The user points Heiwa at storage they already own.
   Preserves local-first fully and adds no Heiwa Limited service. Cost: the user
   configures it, and conflict resolution is still Heiwa's problem.
2. **Heiwa Limited sync service.** An account-backed encrypted state service.
   Matches how ChatGPT and Claude desktop behave and is the least user effort.
   Cost: contradicts the current no-hosted-authority-plane rule, and that rule
   would need to be rewritten deliberately rather than eroded.
3. **Direct device-to-device.** Encrypted peer sync between a user's own
   machines. No third party at all. Cost: hardest to make reliable, and both
   devices must be reachable.

This decision no longer blocks L5. L0 through L4 still keep user state
serializable and scoped to the configuration root so the ratified transport can
carry bounded, redaction-approved projections later.

### D2 — Repository truth update

The single-seat statements in `CLAUDE.md`, `ops/context/HEIWA.md`, and
`docs/current-capability.md` become inaccurate under this document. They should
be revised as part of L0 rather than left to drift, since `docs/current-
capability.md` is the file CI uses to stop public claims from outrunning
verified surfaces.

## Verification

Each layer carries its own gates. Repository-wide, the existing gates continue
to apply: `scripts/check_agent_baseline.sh`, `scripts/check_runtime_baseline.sh`,
`scripts/check_workflow_pins.sh`, and `scripts/audit_product_surface.sh` at zero
unclassified.

- **L0** — the ten surfaces render through the component layer with no behavior
  regression against the current shell; the existing `store.test.ts` and
  `client.test.ts` continue to pass unmodified, proving the operator seam was
  preserved; no path outside the config resolver references a home directory.
- **L1** — a test harness with no provider CLI on `PATH` and a single API key
  completes a turn end to end. This is the fresh-install contract and it must be
  automated, not demonstrated manually.
- **L3** — a connector executes against a live account under approval, and the
  resulting receipt replays from the journal.
- **L4** — an agent action against a user-owned tab is rejected; a T2 action
  without approval is rejected; both produce receipts.

## Sequencing

L0 and L1 are the immediate work and are specified above at implementation
depth. L0 precedes L1 only because the configuration root is L1's dependency;
the UI decomposition and the provider adapters can otherwise proceed in
parallel.

L2 follows, since onboarding is meaningless before a provider works and
required before connectors have an identity to attach credentials to. L3 is the
largest value step and the documented bottleneck. L4 is independent of L3 and
may be scheduled against appetite. Networked L5 follows them; local
machine-perspective bootstrap may land earlier as app-runtime foundation.

The Heiwa Limited release chain runs continuously alongside all of it.
