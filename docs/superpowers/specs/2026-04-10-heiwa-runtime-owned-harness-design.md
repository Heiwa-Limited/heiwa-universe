# Heiwa Runtime-Owned Harness Design

> **Status:** Draft from approved design conversation
> **Date:** 2026-04-10
> **Scope:** `heiwa-universe`, installed `heiwa`, `~/.heiwa/`

## Goal

Turn Heiwa into a real app-owned harness, not a loose collection of repo-local provider configs, imported skills, and partial wrappers.

The harness is the first and most important product.

## One-Sentence Truth

When a user installs `heiwa`, Heiwa creates and owns `~/.heiwa/` as the canonical runtime root, stores provider/model-agnostic harness state there, projects provider-native config outward as needed, and delivers a terse Caveman-style operator experience on top of a Pi-like personal memory layer and a Perplexity Computer-like action/research surface.

## Locked Requirements

The following are fixed by product direction:

- `heiwa` is the app.
- The harness is the first and most important product.
- `heiwa-universe` is the source and deployment monorepo behind the app.
- Installation must feel like a real app bootstrap:
  - `curl https://heiwa.ltd/... | sh`
  - install `heiwa`
  - create `~/.heiwa/`
  - initialize runtime state and generated provider surfaces
- Heiwa must be provider-agnostic and model-agnostic from the user point of view.
- Heiwa must own concise output natively, with strong Caveman influence.
- “Apps” and “skills” should not exist as separate product nouns inside the runtime. Heiwa is the app; internal execution units are capabilities.
- GitHub should present open-source product surfaces honestly, while `heiwa-universe` still deploys enterprise and hosted infrastructure.

## Current Problem

Today’s shape is fragmented:

- `.codex/config.toml`, `.claude/settings.json`, `.gemini/settings.json`, and `~/.heiwa/config.toml` each carry overlapping authority
- provider-native packaging shapes differ by tool and are not generated from one Heiwa source of truth
- imported skills and local plugins are treated too much like product structure
- repo-local configuration still looks more important than the installed runtime
- the public OSS story risks drifting toward internal infra and maturity theater

This is wrong for the product.

Heiwa should not feel like “a repo with some wrappers.” It should feel like “an installed operating layer that can project into provider surfaces when needed.”

## Product Positioning

### Heiwa

Heiwa is the installed operating app and harness.

Its core job is to:

- know the user
- know the machine and fleet
- know connected providers and local runtimes
- know the user’s memory, artifacts, policies, and receipts
- choose the smallest sufficient surface for each task
- call internal capabilities and external provider tools through one coherent runtime

### Heiwa-Universe

`heiwa-universe` remains the source and deployment repo for:

- the installed app
- shared protocol and bindings
- runtime crates and packages
- hosted and enterprise infrastructure
- support automation and operator tooling

But it is not the public product noun.

### Public GitHub Surface

GitHub should emphasize only open-source product surfaces that are real:

- `heiwa` app/runtime
- SDKs and bindings
- install and doctor flows
- documented local-first provider/model support
- self-host or infra components that are honestly usable

GitHub should not foreground:

- customer-specific enterprise overlays
- internal hosted support systems
- half-finished remote control surfaces
- config sprawl as product identity

## Experience Doctrine

### Best of Pi

Heiwa should inherit the useful parts of Pi:

- continuity
- persistent user understanding
- personal context
- calm long-lived relationship memory

This is not a requirement for fluffy output. It is a requirement for stable personal context.

### Best of Perplexity Computer

Heiwa should inherit the useful parts of Perplexity Computer:

- action-oriented computer use
- research plus execution in one surface
- task completion, not just chat
- strong operator confidence that the system can actually do things

### Caveman Output Doctrine

Caveman should be absorbed as native Heiwa response policy, not imported as a vendor plugin.

Default operator behavior:

- terse
- direct
- high signal
- low ceremony
- low repetition
- action over explanation

This must be provider-agnostic:

- Codex
- Claude Code
- Gemini CLI
- Antigravity
- Ollama-backed local work
- future providers

Heiwa should map concise-mode to provider-native controls when available and to prompt/runtime policy when not.

## Canonical Runtime Authority

`~/.heiwa/` becomes the runtime source of truth for the installed app.

Repo-local config files are no longer authorities. At most they are projections or project overlays.

### Authority Order

1. `~/.heiwa/` runtime state
2. project overlays that Heiwa explicitly reads
3. generated provider-native files written by Heiwa
4. provider-owned auth/session state that Heiwa wraps but does not own

### What Heiwa Owns

- runtime config
- routing policy
- provider inventory metadata
- local model inventory metadata
- user modes
- internal capabilities
- hooks and policy logic
- receipts, traces, and artifacts
- generated provider projections

### What Providers Still Own

- native auth/session state
- system prompts
- model inventory semantics
- quotas and billing
- tool semantics inside their own native runtimes

## Canonical `~/.heiwa/` Layout

Proposed layout:

```text
~/.heiwa/
  bin/
    heiwa
  config.toml
  machine.json
  providers/
    registry.json
    claude.json
    codex.json
    gemini.json
    antigravity.json
    ollama.json
  models/
    inventory.json
    routing_classes.json
  capabilities/
    research/
    operator/
    builder/
    review/
    trading/
  modes/
    concise/
    normal/
    explanatory/
    enterprise-safe/
  policies/
    runtime.toml
    safety.toml
    enterprise.toml
  generated/
    codex/
    claude/
    gemini/
    antigravity/
  sessions/
  artifacts/
  logs/
  cache/
  state/
  secrets/
```

Important notes:

- `capabilities/` replaces separate “apps” and “skills” inside the runtime
- `generated/` contains provider-native files Heiwa writes out
- `modes/concise/` or equivalent should be part of the default install
- Ollama does not need a separate projection tree; Heiwa applies policy internally when routing to local models

## Native Capability System

Heiwa should own one internal capability format.

A capability is the internal execution unit used by the harness. It can include:

- manifest
- prompt/instruction assets
- scripts
- tools or MCP dependencies
- policy tags
- evaluation expectations
- UI or presentation metadata if needed

Capabilities are first-class internal runtime resources. DREX can call them directly.

Provider-native `SKILL.md`, plugin, extension, command, or settings files become generated views of capabilities, not the authored source.

This avoids collapsing the product into external packaging systems.

## Provider Projection Contract

### Codex

Heiwa should generate Codex-facing config and native skill projections from `~/.heiwa/generated/codex/` and install or sync them as needed.

### Claude Code

Heiwa should generate Claude-facing settings and plugin/skill projections from `~/.heiwa/generated/claude/`.

### Gemini CLI and Antigravity

Heiwa should generate Gemini-native extension/config projections from `~/.heiwa/generated/gemini/`.

Antigravity should inherit the Gemini projection model where appropriate, while keeping provider identity distinct in Heiwa runtime records.

### Ollama

Ollama remains a local runtime provider. Heiwa applies capability and mode policy internally rather than projecting a third-party config surface.

## Install Contract

`heiwa install` should become the real bootstrap, and the hosted installer should call into that posture.

Install responsibilities:

- create `~/.heiwa/`
- create canonical layout
- write machine identity
- detect available providers and local runtimes
- seed default modes, including concise/Caveman-style output
- seed core capabilities
- generate provider projections
- install canonical launcher
- leave the user in a supportable state where `heiwa doctor` can explain and repair drift

The long-term install path should feel like:

```bash
curl https://heiwa.ltd/install.sh | sh
```

The hosted script should install `heiwa`, then let `heiwa install` perform runtime initialization.

## Repo-Local Files After Refactor

Repo-local provider config files should no longer be hand-authored truth.

Acceptable outcomes:

- generated from Heiwa-owned runtime state
- minimal project overlays consumed by Heiwa
- removed entirely when not required

Not acceptable:

- manually curated repo-local files as the primary product posture
- provider-specific drift that users must reason about directly

## Enterprise and Deployment Boundary

`heiwa-universe` should continue to deploy:

- Railway services
- SpacetimeDB modules and authority plane
- Cloudflare edge/public surfaces
- enterprise policy packs
- hosted sync and control services

But these remain support infrastructure behind the product, not the public product center.

## Migration Plan

### Phase 1: Define runtime-owned schema

- formalize `~/.heiwa/` runtime schema
- define capabilities, modes, providers, models, policies, and generated projections

### Phase 2: Upgrade installer

- expand `heiwa install`
- create full runtime layout
- write default config, modes, and capability seed set

### Phase 3: Generate provider projections

- stop hand-authoring `.codex/config.toml`, `.claude/settings.json`, `.gemini/settings.json` as source
- generate or synchronize outward from `~/.heiwa/`

### Phase 4: Move imported/external skills into native capabilities

- keep useful external ideas
- stop treating external plugin packaging as product structure
- make DREX call native capabilities directly

### Phase 5: Reposition docs and GitHub

- make `heiwa` the product center in docs and OSS presentation
- keep `heiwa-universe` as the engineering/deployment repo behind it

## Success Criteria

The design is successful when all of the following are true:

- a new user can install Heiwa and get a fully initialized `~/.heiwa/`
- Heiwa, not repo-local configs, is the canonical runtime authority
- provider surfaces feel coherent without pretending providers are identical
- concise/Caveman-style output is a native Heiwa mode, not a plugin import
- internal capabilities replace fragmented app/skill vocabulary
- GitHub honestly presents open-source products and avoids enterprise-internal sprawl as the public face
- the harness is unmistakably the first and most important product
