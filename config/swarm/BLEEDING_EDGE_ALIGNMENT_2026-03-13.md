# Heiwa Bleeding-Edge Alignment

Date: 2026-03-13

This memo translates current open-source agentic signals into Heiwa-native moves. The goal is not to mimic hype repos. The goal is to keep Heiwa's March 6 thesis while adopting the subsystems that actually improve speed, reliability, and operator leverage.

## What Heiwa Already Has

- Typed broker routing contracts exist in `packages/heiwa_protocol/heiwa_protocol/routing.py`.
- A unified execution gateway exists in `packages/heiwa_sdk/heiwa_sdk/heiwaclaw.py`.
- SpacetimeDB now owns fast control-plane state in `apps/heiwa_hub/spacetimedb/src/lib.rs`.
- The Python bridge and compatibility DB layer are already moving toward STDB-first reads and writes in `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` and `packages/heiwa_sdk/heiwa_sdk/db.py`.
- Identity-driven routing already exists in `config/identities/profiles.json`.

Heiwa is not missing a thesis. It is missing productized execution layers on top of the thesis.

## Upstream Repo Value

| Repo | Real value for Heiwa | Do not copy | Heiwa-native move |
| --- | --- | --- | --- |
| `msitarzewski/agency-agents` | Role-packaged agents with clear job boundaries | Shallow role cosplay without infra contracts | Build `HeiwaCells`: installable agent packs backed by identities, skills, budgets, and routing policy |
| `promptfoo/promptfoo` | Eval-first prompt and workflow testing, CI integration, red-team loops | Treating eval as prompt-only instead of system-wide | Build `HeiwaBench`: route, tool, model, policy, and workflow evals across identities and providers |
| `666ghj/MiroFish` | Simulation and prediction loops over live external signals | Speculative swarm theater in the hot path | Build `HeiwaPulse`: optional async intelligence feed behind MCP and STDB streams |
| `pbakaus/impeccable` | Opinionated UI steering language and anti-pattern avoidance | Generic AI frontend slop or one-shot page generation | Build `HeiwaUI`: steering commands, design rules, and browser-verified UI harnesses |
| `openviking` | Structured context management and lower-token memory retrieval | File-system memory as the system of record | Build `HeiwaMemory`: STDB-backed summaries, indexes, and workspace materialization caches |
| `p-e-w/heretic` | Adversarial safety research value only | Guardrail removal in operator or public paths | Keep as `lab-only` inspiration for jailbreak testing, never production routing |
| `karpathy/nanochat` | End-to-end small-model literacy, minimal stack understanding | Training hobby projects in the main control plane | Build `HeiwaLab`: isolated local model benchmark and fine-tune lab, not runtime dependency |
| `HKUDS/CLI-Anything` | Adapter/harness generation for making tools agent-native | A plugin shell replacing the Heiwa control plane | Build `HeiwaHarness`: generate fast CLI/MCP adapters that emit state into STDB |

## Strategic Direction

### 1. HeiwaCells

Convert identity profiles plus skill packs into operator-facing agent products.

Definition:
- A `HeiwaCell` is a named, installable agent bundle.
- It includes identity, default models, allowed tools, test pack, cost policy, and UI surface metadata.

Why:
- Heiwa already has identity and routing primitives.
- What it lacks is a clean packaging layer that makes those primitives usable as products instead of internal JSON.

Build from:
- `config/identities/profiles.json`
- `packages/heiwa_skills/*`
- `packages/heiwa_sdk/heiwa_sdk/heiwaclaw.py`

### 2. HeiwaBench

Create a first-class evaluation plane for prompts, routes, tool execution, and policy behavior.

Definition:
- Eval suites for `intent -> broker -> gateway -> tool -> result`.
- Golden tests for route selection, privacy clamps, approval gates, and tool outputs.
- Red-team suites for prompt injection, policy bypass, unsafe tool escalation, and context poisoning.

Why:
- Heiwa currently has good narrow tests and some skill evaluation folders, but not a unified eval product.
- The fastest path to top-tier agentic engineering is to make every important path measurable before it is marketed.

Build from:
- `apps/heiwa_hub/tests/*`
- `packages/heiwa_skills/*/evaluations`
- `packages/heiwa_protocol/heiwa_protocol/routing.py`

### 3. HeiwaMemory

Replace ad hoc context handling with a speed-first memory system built on SpacetimeDB and materialized local caches.

Definition:
- STDB stores compact session, artifact, route, tool, and summary state.
- Workspace caches materialize only the files, summaries, and embeddings needed for the current task.
- Multi-turn sessions stay on WebSockets and STDB subscriptions, not REST polling.

Why:
- Open Viking's core insight is right: context quality controls output quality.
- But filesystem-only memory is too loose for Heiwa's control-plane ambition.

Build from:
- `apps/heiwa_hub/spacetimedb/src/lib.rs`
- `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- `packages/heiwa_sdk/heiwa_sdk/state.py`

### 4. HeiwaUI

Make frontend generation a real subsystem instead of a generic coding side effect.

Definition:
- A steering vocabulary for UI work: simplify, densify, animate, systematize, brand, contrast, compress, dramatize.
- Browser-verified output loops using Playwright.
- Anti-pattern tests for generic layouts, broken hierarchy, poor mobile behavior, and slow runtime surfaces.

Why:
- Impeccable's signal is not "AI makes pretty UIs."
- Its value is an opinionated control surface for UI iteration.

Build from:
- `apps/heiwa_web`
- `packages/heiwa_skills/playwright`
- `packages/heiwa_skills/imagegen`

### 5. HeiwaPulse

Add an optional simulation and market-intelligence plane, but keep it off the critical path.

Definition:
- Ingest radar/trend/news/operator feeds.
- Run async multi-agent simulation or scenario generation.
- Emit summaries, not raw speculative chatter, into STDB.

Why:
- MiroFish-style signal extraction is valuable for strategic sensing.
- It is not acceptable as a mandatory dependency for core execution.

Build behind:
- MCP
- STDB event streams
- `agents-radar`-style feeds

### 6. HeiwaSafehouse

Move from "guardrails exist" to capability leases plus deny-first local sandboxing.

Definition:
- Each tool execution gets an explicit capability lease.
- Writes, network, secrets, shell, and browser rights are scoped per run.
- Unsafe or ambiguous runs fail closed.

Why:
- Heiwa already improved defaults and redaction.
- The next step is execution containment, not more prose about security intent.

Build from:
- `packages/heiwa_sdk/heiwa_sdk/security.py`
- `packages/heiwa_sdk/heiwa_sdk/heiwa_net.py`
- local sandbox rules inspired by agent-safehouse

### 7. HeiwaLab

Create a separate experimental lane for small-model training, benchmarking, and prompt/runtime research.

Definition:
- Benchmark tiny local models, tokenizer choices, quantizations, and latency envelopes.
- Keep training and fine-tuning experiments away from the main production control plane.

Why:
- Nanochat is valuable because it keeps the entire model stack understandable.
- That belongs in an R&D lane, not in the hot path for operator workflows.

## Retire Slow

The following patterns should be actively removed or downgraded:

- REST-only multi-turn agent sessions
- polling loops where STDB subscriptions or WebSockets are possible
- raw SQL in public runtime paths when a typed state service or reducer exists
- placeholder adapters exposed as if they are real capabilities
- filesystem handoff as the primary coordination channel between agents
- long, unstructured prompts pretending to be memory
- provider-specific logic inside agent business logic instead of the gateway

## Non-Adoptions

These signals are useful only as warnings or bounded inspiration:

- `heretic`: use only for safety research, never for normal routing
- generic role-template repos without typed contracts: do not import directly
- frontend one-shot generators without verification: reject
- slow cloud-first orchestration stacks: reject

## 30-Day Build Order

1. Build `HeiwaBench` and make it a release gate.
2. Convert current identities into `HeiwaCells`.
3. Finish the STDB migration for proposals / lease / RFC state and move subscriptions into the live clients.
4. Build `HeiwaMemory` materialization and session-summary indexing.
5. Build `HeiwaHarness` for rapid adapter generation.
6. Build `HeiwaUI` steering + browser verification loop.
7. Add `HeiwaPulse` as an optional async intelligence subsystem.
8. Keep `HeiwaLab` isolated from production.

## SOTA Interpretation For Heiwa

Bleeding edge for Heiwa does not mean "most autonomous."

It means:

- typed contracts over vibes
- evals over belief
- WebSockets over polling
- STDB reducers and subscriptions over compatibility SQL
- installable narrow agent packs over monoliths
- fast local or cheap acceptable routing before premium escalation
- explicit operator control over hidden autonomy

That is the version of AI-native engineering worth building.
