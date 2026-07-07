# Heiwa State, Knowledge, and Build Report

> **Status:** Objective synthesis report
> **Date:** 2026-04-09
> **Scope:** `heiwa-universe`

## Goal

Consolidate the current Heiwa product truth, the April 2026 harness and memory discoveries, and the realistic build sequence into one report that is useful for both product alignment and implementation sequencing.

This document is not a hype memo. It distinguishes:

- what is already real in the repo or local runtime
- what is designed but not yet landed
- what is newly learned and should change the build
- what remains optional, speculative, or explicitly rejected

## Executive Summary

Heiwa’s real product direction is now clear.

Heiwa is a local-first AI operating layer for humans on consumer and edge hardware. It should use the user’s actual local models, cloud entitlements, devices, and durable memory to produce the best achievable result for each task. The key architectural boundary is now explicit:

- model context windows are only working memory
- Heiwa owns durable memory, retrieval, routing, artifacts, and policy
- providers still own their own inference internals and native tools

Three conclusions are now stable:

1. **Memory must become a product subsystem.** Heiwa’s current memory code is too thin. Vault V2 is the right direction: raw-first, per-user, Markdown-first, cross-node, and fed by both conversations and filesystem ingest.
2. **Provider-native sessions must become first-class.** Heiwa should not flatten Claude Code, Codex, and Gemini CLI into dumb prompt pipes. Native-session delegation is the right next orchestration step.
3. **Harness quality comes from evidence, not from bigger prompts.** MemPalace, Anthropic’s harness work, Managed Agents, and Meta-Harness all point in the same direction: keep raw traces, attach small relevant context slices, and let the harness improve itself over time.

The highest-leverage build sequence is:

1. finish provider-truth convergence on `main`
2. land Vault V2 primitives
3. land native session delegation A1/A2
4. add attached context slices and structured task decomposition
5. add Heiwa-native local tool execution for Ollama-tier models
6. build the trace-driven harness optimizer later, on top of the retained corpus

## Canonical Product Truth

The current canonical architecture file is [`HEIWA.md`](../../../HEIWA.md).

The stable product definition is:

> Heiwa is the local-first AI operating layer above local and cloud inference surfaces.

That means:

- `heiwa` is the installed operator surface
- DREX is the routing and execution kernel
- SpacetimeDB is the adjudicated state and evidence plane
- providers still own inference internals
- Heiwa owns orchestration, memory, receipts, policies, and user experience

The most important product doctrines now are:

- **local-first** before remote
- **smallest sufficient surface** before strongest available surface
- **raw truth, derived summaries**
- **working context is not the memory system**
- **provider-native tools when available, Heiwa tools when not**

## Objective State As Of 2026-04-09

### State matrix

| Area                               | State                               | Notes                                                                                                          |
| ---------------------------------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| Canonical architecture identity    | **Real**                            | [`HEIWA.md`](../../../HEIWA.md) now reflects local-first operating-layer truth                                 |
| DREX routing core                  | **Real but incomplete**             | scoring and route planning exist; success-rate feedback is not yet truly fed by native-session outcomes        |
| SpacetimeDB authority plane        | **Real and useful**                 | good substrate for runs, artifacts, loops, task dispatches, leases, worker sessions                            |
| `heiwa` shell/runtime surface      | **Real but narrow**                 | install/doctor/auth/providers/session attach exist; provider semantics are still uneven on `main`              |
| Provider normalization doctrine    | **Designed and partially executed** | implemented in a separate worktree and local `~/.heiwa` state, but not yet merged to `main`                    |
| Local model-first routing doctrine | **Designed**                        | reflected in docs and live local routing, not fully reflected in `main` runtime behavior                       |
| Current memory layer               | **Weak**                            | Python helpers over `captain_*` and embeddings are transitional, not product-grade memory                      |
| Vault V2                           | **Designed**                        | not implemented yet                                                                                            |
| Native session delegation A1/A2    | **Designed**                        | not implemented yet                                                                                            |
| `crates/heiwa_session`             | **Real but primitive**              | existing crate is PTY/socket substrate, not yet the full delegated-session manager described in the A1/A2 plan |
| Meta-harness / trace optimizer     | **Not built**                       | should come later, after traces actually exist at scale                                                        |

### What is real in `main`

Verified from the repo:

- Rust workspace includes `heiwa_core`, `heiwa_shell`, `heiwa_provider`, `heiwa_loop`, `heiwa_session`, `heiwa_stdb`, and generated bindings.
- `heiwa_loop` exists and executes bounded loop turns.
- `apps/heiwa_hub/spacetimedb/src/lib.rs` already contains important reusable primitives:
  - `artifacts`
  - `model_tiers`
  - `loop_sessions`
  - `loop_iterations`
  - `task_dispatches`
  - worker sessions / leases / dispatch acknowledgements
- `apps/heiwa_core/src/drex/router.rs` already uses `last_success_rate` in scoring.

### What is real but still too thin

Verified from code inspection:

- [`crates/heiwa_loop/src/lib.rs`](../../../crates/heiwa_loop/src/lib.rs) still sends one objective per turn and ignores `StreamEvent::ToolUse`.
- [`crates/heiwa_provider/src/adapter.rs`](../../../crates/heiwa_provider/src/adapter.rs) still exposes only the stateless `ProviderAdapter`.
- [`crates/heiwa_provider/src/providers/claude_code.rs`](../../../crates/heiwa_provider/src/providers/claude_code.rs) is still a one-shot CLI adapter.
- [`crates/heiwa_session/src/lib.rs`](../../../crates/heiwa_session/src/lib.rs) is a minimal socket/PTTY substrate, not yet a delegated-session store/manager.
- [`crates/heiwa_provider/src/detect/mod.rs`](../../../crates/heiwa_provider/src/detect/mod.rs) on `main` is still pre-normalization and Ollama-centric.
- [`apps/heiwa_shell/src/main.rs`](../../../apps/heiwa_shell/src/main.rs) on `main` still uses older provider assumptions such as loop capability for `"claude"` / `"ollama"` rather than the newer execution-surface vocabulary.

### What is real locally but not yet merged

Outside `main`, but already executed in the local environment:

- the live `~/.heiwa` routing state has been normalized around real execution surfaces
- local routing currently prefers:
  - `ollama/qwen3.5:9b` for code
  - `ollama/gemma4:latest` for chat
  - `ollama/qwen3-embedding:0.6b` for embeddings
  - `gemini-cli/gemini-3.1-pro` for remote reasoning
  - `claude/sonnet-4-6` for review

This is useful, but the report must stay honest:

- **live local state is ahead of repo `main`**
- **doctrine is ahead of implementation**

## Knowledge That Now Feels Stable

This section translates the large information dump into durable Heiwa knowledge.

### 1. Working context versus memory is now a solved boundary

The user’s framing is correct:

- each model deployment has a native context window
- that context window is only working context
- Heiwa’s harness and memory system decides what slice enters that window

This is now stable doctrine:

- Heiwa memory is the durable external memory system
- the harness produces an `attached_context_slice`
- the model spends its working context on active reasoning and tool use

This boundary prevents a major design mistake: pretending “larger context windows” remove the need for a serious memory system.

### 2. MemPalace changes Heiwa’s memory strategy, not its authority plane

What MemPalace proves:

- raw-first memory is superior to summary-first memory
- filesystem + conversation ingest is necessary
- layered retrieval is more efficient than loading giant context every time
- compact wake-up context is useful when derived from raw truth

What it does **not** prove:

- that Heiwa should replace SpacetimeDB with ChromaDB/SQLite
- that compression should become canonical storage

Heiwa translation:

- Vault V2 should be raw-first and user-scoped
- conversations and files must both land from day one
- wake-up context should be tiny and derived
- Heiwa keeps STDB as the authority plane

### 3. Anthropic’s harness work validates planning, artifacts, and specialized roles

The useful lessons are:

- long tasks should be decomposed
- planning and execution should not be one undifferentiated loop
- artifacts are the right handoff unit
- evaluator passes should exist, but only where they justify their cost
- harnesses must be periodically simplified as models improve

Heiwa translation:

- task specs must be explicit
- planner / executor / evaluator should become distinct roles
- evaluator work should be policy-gated
- harness components must remain removable, not sacred

### 4. Managed Agents validates brain/hand separation

This is highly compatible with Heiwa:

- brains are model deployments chosen per step
- hands are tools, shells, worktrees, devices, MCP servers, and remote nodes
- the harness routes between brains and hands

This should inform both provider-native session delegation and the future Heiwa-native tool loop.

### 5. Meta-Harness validates trace retention as a first-class system requirement

The strongest Meta-Harness lesson is not “use this benchmark.”

It is:

- raw traces beat summary-only hindsight
- filesystem-retained code/log/score corpora enable true harness optimization

Heiwa translation:

- delegated sessions must retain raw event logs, outputs, diffs, receipts, and evaluation notes
- `~/.heiwa/sessions/<id>/` is not just debugging residue; it is future optimizer input

### 6. Token-efficiency advice is partly real and partly folklore

The useful parts:

- concise output styles are often better
- persistent memory beats repeating setup messages
- project retrieval beats re-uploading the same context
- batching related asks is better than many tiny follow-ups

The parts that should **not** become Heiwa doctrine:

- quota/window gaming hacks
- undocumented provider-specific tricks
- architecture shaped around one vendor’s current billing behavior

Heiwa translation:

- support provider-agnostic verbosity profiles
- map to provider-native styles when available
- never make provider quota gaming part of core architecture

### 7. Awesome Harness Engineering is a backlog map, not a truth source

Its best value for Heiwa is as a taxonomy and periodic research queue:

- tool risk metadata
- memory governance
- structured local outputs
- sandboxing and config protection
- explicit harness artifacts

It should not be treated as a design authority on its own.

## Current Heiwa Knowledge Stack

At this point, Heiwa’s knowledge stack should be understood like this:

### Layer 1: Product truth

- local-first operating layer
- best achievable result per task
- smallest sufficient surface first

### Layer 2: Durable memory truth

- per-user vault
- per-project and per-topic retrieval
- cross-node replication
- user-visible Markdown workspace

### Layer 3: Task execution truth

- scoped task specs
- provider-native sessions where they exist
- Heiwa-native tools where they do not
- structured artifacts and receipts

### Layer 4: Harness optimization truth

- retain traces
- score outcomes
- feed routing reliability
- later optimize the harness itself from the trace corpus

## What Heiwa Should Build Next

This is the realistic build order if the goal is “best AI value on consumer and edge hardware.”

### Step 0: converge doctrine and repo truth

Before anything else:

- re-land provider normalization from the existing worktree into `main`
- align `heiwa_shell`, `heiwa_provider`, and discovery logic with execution-surface vocabulary
- make `main` match the routing doctrine already proven in local `~/.heiwa`

Reason:

- if provider truth is wrong, later memory and session layers inherit false assumptions

### Step 1: land Vault V2 primitives

Build next:

- `vault_*` STDB tables and reducers
- Rust-first vault crate and local read model
- conversation + filesystem ingest
- wake-up context generation

Reason:

- without durable memory, every session and agent remains context-starved
- this is the foundation for `attached_context_slice`

### Step 2: land native session delegation A1/A2

Build next:

- `SessionProvider` alongside `ProviderAdapter`
- local-first session receipts under `~/.heiwa/sessions`
- Claude Code pilot
- task-dispatch + artifact projection

Reason:

- this unlocks the native power of Claude/Codex/Gemini instead of flattening them into prompt pipes

### Step 3: add `attached_context_slice`

Build next:

- per-user and per-project retrieval policy
- context selection sized to each model’s working window
- provider-agnostic context attachment rules

Reason:

- this is the actual boundary between durable memory and working context
- without it, Vault V2 exists but is not yet operationally useful

### Step 4: add A3 decomposition and evaluation policy

Build next:

- task decomposition
- planner / executor / evaluator roles
- evaluator gating by risk and task difficulty

Reason:

- native sessions alone are not enough
- Heiwa must decide when to keep work local, when to escalate, and when to run review/evaluator passes

### Step 5: build the Heiwa-native local tool loop

Build next:

- local tool execution for `qwen3.5` / `gemma4` tier work
- structured output paths for local models
- low-cost local verification and repair loops

Reason:

- local models must do more than summarize if Heiwa is going to be truly cost-efficient

### Step 6: build the trace-driven harness optimizer

Build later:

- trace search over retained session corpora
- harness critique and redesign loops
- routing policy refinements driven by actual outcomes

Reason:

- this is high value, but only after traces, tasks, memory, and delegated sessions are real at scale

## Anti-Goals

Heiwa should explicitly avoid the following failure modes:

- replacing STDB authority with local sidecars
- making remote frontier sessions the default path
- treating compression as canonical memory
- building around provider-specific limit hacks
- burying orchestration state in giant prompts instead of explicit artifacts
- claiming that design docs already equal working product surfaces

## Objective Summary Of State

### State

- Heiwa has a real local-first architecture and a real Rust/STDB substrate.
- `main` is still behind the latest doctrine in provider normalization, memory, and provider-native session orchestration.
- the local machine state has already validated a better routing truth than the repo currently encodes.

### Knowledge

- raw-first memory is the correct memory direction
- native context windows are working memory, not the memory system
- provider-native sessions are worth preserving
- structured artifacts are the right handoff protocol
- full trace retention is essential for later harness optimization
- local models should be the default tier whenever quality allows

### Build steps

1. merge provider truth
2. implement Vault V2 foundation
3. implement native session delegation A1/A2
4. implement attached context slices
5. implement decomposition + evaluation policy
6. implement local tool loop
7. implement trace-driven harness optimization

## Source Documents Reviewed

Repo-local:

- [`HEIWA.md`](../../../HEIWA.md)
- [`docs/routing.md`](../../../docs/routing.md)
- [`docs/current-capability.md`](../../../docs/current-capability.md)
- [`2026-04-08-heiwa-vault-v2-design.md`](./2026-04-08-heiwa-vault-v2-design.md)
- [`2026-04-08-heiwa-native-session-delegation-design.md`](./2026-04-08-heiwa-native-session-delegation-design.md)
- [`2026-04-09-heiwa-harness-optimization-design.md`](./2026-04-09-heiwa-harness-optimization-design.md)

External:

- [MemPalace](https://raw.githubusercontent.com/milla-jovovich/mempalace/main/README.md)
- [Anthropic harness design](https://www.anthropic.com/engineering/harness-design-long-running-apps)
- [Anthropic managed agents](https://www.anthropic.com/engineering/managed-agents)
- [Meta-Harness](https://yoonholee.com/meta-harness/)
- [caveman](https://github.com/JuliusBrussee/caveman)
- [awesome-harness-engineering](https://github.com/ai-boost/awesome-harness-engineering)
- [Claude styles](https://support.claude.com/en/articles/10181068-configure-and-use-styles)
- [Claude memory](https://support.claude.com/articles/11817273-how-does-claude-s-memory-work)
- [Claude Projects RAG](https://support.claude.com/en/articles/11473015-retrieval-augmented-generation-rag-for-projects)
- [Claude Code plan usage](https://support.claude.com/en/articles/11145838-use-claude-code-with-your-pro-or-max-plan)
