# Heiwa Harness Optimization Design

> **Status:** Draft based on April 9, 2026 research
> **Date:** 2026-04-09
> **Scope:** `heiwa-universe`

## Goal

Translate the most useful current discoveries in agent memory, long-running harness design, and token-efficiency into Heiwa-native internal doctrine.

This document is not a vendor fan note. It separates:

- ideas that should become Heiwa architecture
- ideas that should remain optional experiments
- ideas that should be rejected as product doctrine

## One-Sentence Truth

Heiwa should behave like a meta-harness for real users on consumer and edge hardware: keep raw evidence, route each step to the smallest sufficient surface, preserve provider-native strengths, and continuously improve its own orchestration from retained traces and outcomes.

## Research Inputs

Primary or near-primary sources reviewed on 2026-04-09:

- MemPalace README and benchmarks
- Awesome Harness Engineering: [awesome-harness-engineering](https://github.com/ai-boost/awesome-harness-engineering)
- Anthropic: [Harness design for long-running application development](https://www.anthropic.com/engineering/harness-design-long-running-apps)
- Anthropic: [Scaling Managed Agents: Decoupling the brain from the hands](https://www.anthropic.com/engineering/managed-agents)
- Meta-Harness project: [Meta-Harness](https://yoonholee.com/meta-harness/)
- Caveman README: [caveman](https://github.com/JuliusBrussee/caveman)
- Claude Help Center:
  - [Retrieval augmented generation (RAG) for projects](https://support.claude.com/en/articles/11473015-retrieval-augmented-generation-rag-for-projects)
  - [Use Claude’s chat search and memory to build on previous context](https://support.claude.com/articles/11817273-how-does-claude-s-memory-work)
  - [Configure and use styles](https://support.claude.com/en/articles/10181068-configure-and-use-styles)
  - [Use Claude Code with your Pro or Max plan](https://support.claude.com/en/articles/11145838-use-claude-code-with-your-pro-or-max-plan)
  - [Claude March 2026 usage promotion](https://support.claude.com/en/articles/14063676-claude-march-2026-usage-promotion)

## How To Use Awesome Harness Engineering

`awesome-harness-engineering` is useful as a taxonomy and bibliography, not as a canonical architecture source on its own.

What it adds for Heiwa:

- a clean breakdown of harness scope: context delivery, planning artifacts, tools, memory, sandboxes, verification, tracing, orchestration
- a good map of adjacent work worth mining later
- a reminder that harness components should be expected to shrink or disappear as models improve

What it should not become:

- a cargo-cult checklist where Heiwa copies every listed pattern
- a substitute for primary-source design decisions

The repo is best treated as:

- source map
- backlog generator
- periodic review list for new harness ideas

## High-Value Additions From The List

Beyond the primary sources already captured elsewhere, the list surfaces several items that are especially relevant to Heiwa.

### 1. Tool risk vocabulary

The MCP tool-annotation framing is useful because risk belongs at the harness boundary, not only inside prompts.

Heiwa translation:

- tools should carry risk metadata
- DREX and execution policy should reason over tool combinations, not isolated tools
- permissioning should be structural, not conversational

### 2. Memory governance

The memory-governance references in the list reinforce a gap that Heiwa must close: durable memory needs lifecycle policy, not just storage.

Heiwa translation:

- Vault V2 needs conflict, privacy, and staleness handling
- “zombie memory” is a harness problem, not a model problem
- write policies matter as much as retrieval policies

### 3. Structured output enforcement for local models

The list’s structured-output tools are relevant because Heiwa will rely heavily on local models for low-cost work.

Heiwa translation:

- local Qwen/Gemma paths should prefer schema-constrained outputs for routing, extraction, and evaluator steps
- structured decoding is part of making small local models useful inside a larger harness

### 4. Security and sandbox doctrine

The list’s sandbox references reinforce an important Heiwa rule: an agent must not be able to rewrite its own harness boundary.

Heiwa translation:

- protect MCP configs, hooks, router policy, and provider config from agent mutation
- isolate untrusted execution from the operator workspace
- sandbox design must account for tool combinations and config tampering

### 5. Natural-language harness artifacts

The list’s natural-language harness references align with a strong Heiwa direction: plans, policies, and operating artifacts should be explicit and versioned.

Heiwa translation:

- execution artifacts should be inspectable and replayable
- task specs, plan fragments, evaluator rubrics, and routing reasons should not stay buried in controller code
- this makes future harness self-optimization much easier

## What Is Actually New And Useful

### 1. MemPalace: raw-first memory beats summary-first memory

What holds up:

- raw verbatim storage is the performance center
- conversations and filesystem mining both matter
- layered recall is cheaper than loading giant context every time
- structure matters when it narrows search before semantic ranking
- compression is optional and must not replace canonical storage

What does not hold up as doctrine:

- treating AAAK as the default path
- marketing claims that imply compression is already the retrieval moat

Heiwa translation:

- Vault V2 stays raw-first
- canonical storage remains durable and verbatim
- wake-up context stays tiny and derived
- compression layers are read models, not truth

## 2. Anthropic long-running harness design: planning and structured handoff are load-bearing

What holds up:

- decompose long work into tractable chunks
- use structured artifacts to hand off context between agents or sessions
- planner/generator/evaluator is useful when the task sits beyond solo model reliability
- evaluator cost is conditional, not universal
- as models improve, some harness layers stop being load-bearing and should be removed

Heiwa translation:

- a scoped task spec is mandatory before delegated execution
- artifacts are first-class internal currency
- planner, executor, evaluator are separate roles, not one giant loop
- evaluator should be policy-controlled and only turned on when the task warrants it
- harness modules must be swappable and removable as model capability changes

## 3. Managed Agents: separate brains from hands

What holds up:

- reasoning surfaces and execution environments should be decoupled
- tools should look like generic hands, not provider-specific magic
- many brains and many hands are more scalable than one monolithic session

Heiwa translation:

- DREX chooses the brain for a step
- tools, shells, MCP servers, worktrees, Railway nodes, and local devices are hands
- no single provider session should own the whole system state
- provider-native sessions remain important, but Heiwa owns the orchestration fabric around them

## 4. Meta-Harness: full traces beat summary-only optimization

What holds up:

- harness optimization requires raw code, logs, scores, and traces
- summary-only hindsight is too lossy for many failures
- filesystem-retained artifacts enable counterfactual diagnosis and targeted harness edits

Heiwa translation:

- every delegated session must leave a durable local trace corpus
- provider events, diffs, evaluation notes, screenshots, logs, and receipts must be queryable later
- session storage is not just for audit; it is future training data for the harness optimizer
- Heiwa should eventually support self-optimization over its own trace filesystem

This is why `~/.heiwa/sessions/<id>/` is not incidental plumbing. It is the beginning of a Heiwa-native meta-harness corpus.

## 5. Token-efficiency advice: some is real doctrine, some is folklore

### Real doctrine

These are worth translating into Heiwa:

- shorter output styles often save cost and improve readability
- provider-visible style controls should be used when available
- memory and retrieval beat repeating setup context in every chat
- project-scoped retrieval is better than re-uploading the same context repeatedly
- batching related work into one scoped task is better than many shallow follow-ups

Heiwa translation:

- add provider-agnostic verbosity profiles such as `concise`, `normal`, `explanatory`
- map those profiles to provider-native styles when available and prompt guidance otherwise
- prefer structured artifacts over repeated prose explanations between steps
- keep persistent per-user and per-project memory so the same context is not re-sent forever

### Not doctrine

These should not become core Heiwa behavior:

- provider-specific quota gaming like “warm up the window at 6:15 AM”
- undocumented billing or limit hacks
- assumptions that a user’s optimal day is defined by one vendor’s current quota mechanics

Reason:

- they are not portable
- they may stop working without notice
- they are operational heuristics, not architecture

At most, Heiwa may expose them as optional provider tips if they are documented and current.

## Load-Bearing Heiwa Doctrines

From the research above, the following are now architecture doctrine.

### Doctrine 0: model context is working memory, not the memory system

Every provider/model deployment comes with a native context window. That window is the model’s working context, not the system’s durable memory.

Heiwa owns the larger context system around it:

- per-user memory
- per-project memory
- trace/artifact recall
- retrieval and handoff policy

Operational rule:

- the harness retrieves a small, relevant slice from Heiwa memory
- that slice is attached to the active agent/session
- the model then spends its working context on active reasoning and tool use

The objective is not “fit everything into context.” The objective is “feed the model the right slice so its limited working context is used efficiently.”

### Doctrine 1: Raw truth, derived summaries

- Vault stores raw conversations and files
- session storage keeps raw traces
- summaries are caches and handoff artifacts, never the only truth

### Doctrine 2: Smallest sufficient surface

- `qwen3.5:4b`, `qwen3.5:9b`, and `gemma4` do default work whenever quality is acceptable
- cloud or session-native surfaces are escalation paths
- the point is the best result per dollar, per watt, and per minute

### Doctrine 3: Artifacts are the handoff protocol

- no free-form “what happened?” blobs when a structured artifact will do
- plans, critiques, diffs, logs, scorecards, and receipts should be explicit records
- context transfer should prefer artifact references over giant replay prompts

### Doctrine 4: Brain-hand separation

- brains choose, plan, and critique
- hands execute against shells, filesystems, browsers, MCP tools, worktrees, and remote nodes
- Heiwa routes between them

### Doctrine 5: Harnesses must be self-improving

- retain traces locally
- score outcomes
- update routing reliability from outcomes
- later: search traces to redesign the harness itself

## Concrete Translation To Current Heiwa Work

### A1/A2 native session delegation

Required now:

- preserve `provider_events.jsonl`, stdout/stderr, usage, patch, and review state
- keep local session directories as optimizer-readable trace corpora
- ensure write sessions produce explicit diff artifacts

### Vault V2

Required now:

- keep raw documents and transcripts canonical
- support both filesystem and conversation ingest from day one
- generate tiny wake-up context from raw memory instead of summarizing away the source
- support per-user attached context slices so every active agent starts with the right memory, not the whole vault

### DREX

Required now:

- route by smallest sufficient surface
- prefer local Qwen/Gemma for prep, summarization, verification, and bounded coding
- escalate only when reasoning depth, native tool use, or accuracy requires it
- absorb success/failure feedback from delegated sessions

### Future A3/C/B work

Required later:

- planner/generator/evaluator roles
- optional evaluator passes gated by task difficulty
- harness trace search and replay
- Heiwa-native tool loop for local models
- compression profiles for operator-visible output and memory views

## New Internal Concepts

These should become explicit nouns in future implementation.

### `verbosity_profile`

Provider-agnostic output profile:

- `concise`
- `normal`
- `explanatory`

Map to:

- provider-native styles where available
- prompt directives where not

### `trace_corpus`

The local filesystem bundle of:

- task spec
- provider events
- logs
- artifacts
- screenshots
- diffs
- evaluation scorecards
- receipts

### `evaluation_policy`

Policy for whether a task gets:

- no evaluator
- lightweight evaluator
- full planner/generator/evaluator loop

### `compression_layer`

Derived representation for:

- wake-up context
- brief summaries
- memory handoff

Never the primary truth store.

### `attached_context_slice`

The selected subset of Heiwa memory attached to one active model/session:

- small enough to fit comfortably inside the model’s working context
- rich enough to prevent repeated setup chatter
- derived from user, project, and task state

This is the core harness boundary between durable memory and model-time reasoning.

## What Heiwa Should Explicitly Not Do

- do not replace provider-native memory with provider-specific folklore
- do not treat compression as memory truth
- do not build the whole system around Claude-specific quota behavior
- do not make remote frontier sessions the default just because they feel stronger
- do not discard raw traces in favor of short summaries

## Decision

The correct translation of April 2026 discoveries into Heiwa is:

1. keep MemPalace-style raw-first memory
2. keep Anthropic-style structured planner/executor/evaluator roles
3. keep Managed-Agents-style brain/hand separation
4. keep Meta-Harness-style full trace retention for later optimizer loops
5. adopt terse, reusable context and output profiles
6. reject provider-specific billing hacks as product doctrine

That is the real architecture upgrade, not copying one repo or one vendor workflow whole.
