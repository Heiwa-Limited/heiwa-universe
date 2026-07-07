# Heiwa Brand Reference

Updated: 2026-04-21
Status: working brand and positioning anchor for `heiwa.ltd`, docs, launch copy, and design handoffs

## Purpose

This file defines the smallest stable public truth for Heiwa's brand, messaging, and visual direction.

Use it when writing:

- `heiwa.ltd` landing and download pages
- `/providers` and `/vs/*` comparison pages
- docs intros and release copy
- future designer or Claude Design handoffs

If marketing copy conflicts with [`HEIWA.md`](/Users/dmcgregsauce/heiwa-universe/HEIWA.md), `HEIWA.md` wins.

## One-Line Product Truth

**Heiwa turns the subscriptions and models you already have into a local AI runtime, with routing, evidence, and operator control.**

Short variants:

- Reuse the AI subscriptions you already pay for.
- Run local-first. Escalate only when needed.
- One operator runtime across local models, OAuth CLIs, and API-key providers.

## What Heiwa Is

Heiwa is not just a router.

Heiwa is a local-first operator runtime with four attached capabilities:

1. runtime
2. routing
3. memory and evidence
4. orchestration

Public compression:

> `heiwa` is the installed operator surface.\
> DREX is the kernel.\
> SpacetimeDB adjudicates and records.\
> Providers still own inference.

## What Heiwa Is Not

Do not market Heiwa as:

- a browser-first coding IDE
- a thin OpenRouter clone
- a generic BYOK proxy
- a hosted multi-tenant SaaS control panel
- a hosted operator app we run for users
- a service where sessions, evidence, or provider tokens live on Heiwa's servers
- a claim that all providers have equal execution depth today

Heiwa does not host the runtime, the app, the REPL, the memory, or the operator's secrets. Users install everything locally. We ship binaries, docs, and identity exchange — they run the system.

## Core Positioning

Heiwa owns a distinct corner:

- **You host everything:** the runtime, the app, the REPL, the memory, and the secrets all live on the operator's machine. Heiwa ships software, not a service.
- **Subscription reuse via OAuth:** the AI subscriptions the operator already pays for — Claude Max, ChatGPT Plus, Google AI Pro — plug in through OAuth-backed provider CLIs, alongside API keys and local Ollama, under one surface
- **Local-first runtime:** work stays on the operator's machine by default; remote providers are escalation surfaces
- **Operator-grade execution:** REPL, loops, approvals, subagents, routing, receipts, and evidence belong in the same product

Category sentence:

> Heiwa is the sovereign operator's local AI runtime.

## Audience

Primary audience:

- technical operators
- founders
- staff-level builders
- infra-minded individuals who already pay for AI tools and want one coherent runtime

They care about:

- reusing existing subscriptions
- cost control
- local privacy
- visible routing decisions
- auditable execution
- not getting trapped in one provider's UX

## Message Hierarchy

Lead in this order:

1. **Reuse what you already pay for**
   - Claude Code, Gemini CLI, Codex, Antigravity, API-key providers, and local Ollama can live behind one operator surface
   - do not imply equal maturity; say "supported surfaces" or "connected providers" when needed
2. **Local-first by default**
   - local models and local execution are the working tier
   - remote providers are escalation surfaces
3. **Receipts, routing, and control**
   - approvals, traces, evidence, and operator visibility are product features, not side effects

Do not lead with:

- backend topology
- SpacetimeDB internals
- Railway
- abstract "agent platform" language
- broad enterprise claims before the operator story is clear

## Homepage Messaging Shape

Hero:

- headline should lead with subscription reuse and local runtime
- subhead should add routing, control, and receipts
- CTA should bias toward install and provider compatibility, not "book a demo"

Recommended headline territory:

- Reuse the AI subscriptions you already pay for.
- Your local AI runtime for subscriptions, local models, and receipts.
- One operator runtime across Claude Code, Gemini, Codex, Ollama, and keys.

Recommended supporting sentence territory:

- Heiwa unifies local models, OAuth CLI providers, and API-key providers into one local-first operator surface with routing, approvals, and evidence.

## Honest Claims

Safe strong claims:

- local-first
- self-hosted by default — the operator runs the runtime, the app, and the REPL on their own machine
- installed `heiwa` runtime is the product center of gravity
- provider-owned CLIs are wrapped, not replaced
- OAuth-backed subscription reuse (Claude Max, ChatGPT Plus, Google AI Pro, Copilot) is a first-class provider lane
- local models are first-class
- routing and evidence are explicit product concerns
- Heiwa helps operators reuse existing subscriptions and accounts

Claims that require care:

- "all providers in one place" only if paired with maturity qualifiers
- "full orchestration" only when the surface being discussed actually has it
- "cloud" language only for delivery surfaces (`heiwa.ltd` static pages, release hosting, OAuth callback exchange); never for runtime or user data

Avoid:

- "fully autonomous" on marketing pages
- "replace every AI app"
- "unlimited"
- "enterprise-ready" without scoped proof
- fake parity language

## Competitor Framing

Named competitors matter because users need orientation fast.

Use comparison pages such as:

- `Heiwa vs Manifest`
- `Heiwa vs OpenRouter`
- `Heiwa vs LiteLLM`

The framing should be direct and specific:

- Manifest is closer on operator UX and self-serve packaging
- OpenRouter is a remote model gateway, not a local operator runtime
- LiteLLM is a useful compatibility/proxy layer, not the same product category

Heiwa's strongest differentiators:

- local-first runtime instead of remote-first gateway
- OAuth CLI / subscription reuse as a first-class provider lane
- operator receipts, approvals, and evidence
- one surface across local models, provider CLIs, and key-based providers

## Surface Architecture For Brand Work

Public surfaces should be described this way:

- `heiwa.ltd`: marketing, install, docs, comparison pages, release delivery, OAuth callback exchange — static + edge only, no runtime, no user data
- `heiwa`: primary installed runtime, REPL, and local server the operator runs on their own machine
- operator app: served by the local runtime on `localhost:<port>`, opened via `heiwa app` — not a hosted subdomain

Brand copy should keep the distinction clear:

- the website is the shop window
- the installed runtime is the product center
- the app is an attached operator surface served locally by the runtime, not a hosted product Heiwa runs

Do not refer to a hosted `app.heiwa.ltd` as if it were the operator product. If a hosted surface under `heiwa.ltd` ever exists, it is documentation, interactive demos, or the OAuth callback relay — never the operator's session or data.

## Voice

Voice traits:

- exact
- operator-literate
- calm
- direct
- technically grounded

Avoid:

- hype language
- startup clichés
- mystical AI phrasing
- fake simplicity claims
- vague "platform" copy with no operator nouns

Preferred nouns:

- operator
- runtime
- provider
- route
- evidence
- approval
- receipt
- session

## Visual Direction

The marketing site should not look like the current cockpit pages with lighter copy pasted on top.

Direction:

- lighter, clearer, more spacious than the operator cockpit
- still technical, but less dashboard-coded
- use the existing orange/teal accents selectively
- dark can stay, but it needs more contrast hierarchy and calmer density

Reference mood:

- product confidence from Linear
- restraint from Vercel and Val Town
- none of the heavy observability-tool clutter

Do not visually imply:

- NOC dashboard
- crypto casino
- generic AI gradient slop

## Distribution Truth

Distribution is part of the product, not an afterthought. Because Heiwa ships software instead of hosting a service, the install path IS the onboarding experience.

Priority surfaces:

1. one-line install
2. GitHub Releases
3. provider compatibility table with OAuth, API key, and local lanes
4. comparison pages
5. docs that explain the local-first, self-hosted model clearly

The story is not "look how complex the architecture is."

The story is:

> You already pay for powerful AI tools. Heiwa turns them into one local operator runtime you host yourself.

## Copy Guardrails

When writing copy, preserve these boundaries:

- providers own auth and inference internals
- Heiwa owns sessions, routing, sandboxes, evidence, and operator coherence — **on the operator's machine**
- the operator hosts the runtime, the app, and the REPL; Heiwa does not host them
- `heiwa.ltd` delivers software, docs, and identity exchange; it does not execute operator work or store operator data
- local-first is the default posture
- remote execution is an escalation path
- present maturity honestly by surface

## Immediate Deliverables This File Should Drive

1. `heiwa.ltd/` landing copy
2. `/providers` comparison schema with `local`, `api_key`, and `oauth_cli` lanes
3. `/vs/manifest`, `/vs/openrouter`, `/vs/litellm` pages
4. `/download` copy centered on install and release distribution
5. future design prompts that need stable product truth and tone
