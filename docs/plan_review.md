# PLAN.md Review

**Date:** 2026-03-23
**Reviewer:** Claude Code (Class 3 Executor)
**Document:** `PLAN.md` — Heiwa Inspired-by-Pi Roadmap

---

## Verdict: Strong plan, well-grounded in current architecture

The roadmap correctly identifies what to take from pi-mono and what to leave behind. It builds on real existing infrastructure rather than proposing rewrites. The phasing is sound. Below are findings organized by section.

---

## Section 1: Agent Standards — First-Class Contract

### Strengths

- Correctly identifies the current fragmentation: standards are split across `CLAUDE.md`, `AGENTS.md`, `HEIWA.md`, and room docs.
- "Treat the standard as part of the runtime contract, not just contributor docs" is the right framing — standards that aren't enforced at runtime are aspirational at best.
- HeiwaBench validation of loaded standards/config is a smart gate.
- Preserving Heiwa's model routing philosophy rather than importing pi-mono conventions blindly is the right call.

### Concerns

- **Scope creep risk.** "One canonical document" that encodes typing expectations, verification commands, commit logging, tool execution rules, file mutation rules, _and_ risky action rules is a lot. Consider whether this is one document or one _contract_ backed by multiple focused documents. A single monolith becomes stale faster than a composed contract.
- **Enforcement mechanism unspecified.** The plan says the standard is a runtime contract but doesn't say how it's enforced. Is it loaded as context for Class 3 agents? Validated by a HeiwaBench suite? Both? Specify the enforcement path.
- **Missing: versioning.** If the agent standard is a runtime contract, it needs a version field. Otherwise you can't tell which standard a task was executed against.

### Recommendation

Add a sub-bullet clarifying enforcement: "The standard is loaded as agent context at task start AND validated by HeiwaBench at release gates." Add a version field to the standard contract.

---

## Section 2: Tool Lifecycle Hooks and Lease-Gated Execution

### Strengths

- Correctly identifies the two actual choke points: OpenClaw dispatch and ToolMesh wrapper invocation. These are the right places for hooks — not in BaseAgent (which should stay thin).
- Hook context spec is thorough: proposal/task IDs, worker, tool name, parsed args, lease metadata, risk classification.
- `before_tool_call` response types (allow/deny/approval_required/modified_context) cover the real scenarios.
- `after_tool_call` responsibilities (audit, redaction, STDB recording) are well-scoped.
- "Move lease handling from 'state exists' to 'execution denied without a valid lease'" is the critical shift. Today leases exist in STDB but are never checked at execution time.
- Keeping BaseAgent thin with optional agent-level hook helpers later is architecturally correct.

### Concerns

- **Performance impact unaddressed.** Every tool execution now requires a lease lookup (STDB round-trip via CLI subprocess). The current `spacetimedb.py` bridge uses `spacetime sql` subprocess calls with retry logic. At high throughput, this becomes a bottleneck. The plan should acknowledge this and either:
  - Accept it for now (low throughput reality), or
  - Plan for a lease cache with subscription-based invalidation.
- **Hook ordering and failure semantics.** What happens when `before_tool_call` itself fails (not denies — crashes)? Fail-closed (deny) or fail-open (allow)? This needs to be specified. Fail-closed is the right answer but should be explicit.
- **`modified_context` in before_tool_call is risky.** Allowing hooks to mutate execution metadata opens a wide surface for bugs. Consider restricting this to "append metadata" rather than "mutate arbitrary fields." The plan says "only if explicitly permitted" but doesn't define who permits it or how.
- **Lease scope matching specifics.** The plan says "lease scope must be checked against tool, filesystem, network, and secret scopes already stored in STDB." The schema for `capability_leases` already has these fields (`tool_scope`, `filesystem_scope`, `network_scope`, `secret_scope`). Good. But the matching semantics aren't defined:
  - Is `tool_scope` an exact match or a prefix/glob?
  - Is `filesystem_scope` a path prefix?
  - What does "matching" mean for `network_scope` and `secret_scope`?
  - These need to be specified before implementation.

### Recommendations

1. Add a "Failure mode" bullet: `before_tool_call` failures are fail-closed (deny with error reason).
2. Restrict `modified_context` to metadata-append-only in the first iteration.
3. Define lease scope matching semantics (exact, prefix, glob) for each scope field.
4. Acknowledge the STDB round-trip cost and plan for a lease cache when throughput demands it.

---

## Section 3: Operator UI Subsystem

### Strengths

- "Do not start with Rust" is the right decision. The pi-mono comparison doc (`docs/pi_mono_comparison.md`) referenced a proposed UI package that is now legacy/quarantined unless promoted under current product doctrine.
- The pi-tui concepts to borrow are well-chosen: differential redraw, overlay interaction, focus/input handling, event/render separation.
- "Shared UI data model, not one renderer for every surface" is the correct abstraction — Discord and terminal renderers consume the same state model.
- The milestone targets (route/runtime/model, lease status, approval state, worker availability, task events) are the actual operator needs.

### Concerns

- **No concrete first deliverable.** "The first milestone should improve Heiwa's operator surfaces" is vague. What does the operator see on day one of the new UI? A status panel? A lease dashboard? A live event stream? Pick one and ship it.
- **Python TUI library choice unspecified.** Textual? Rich? Custom? This matters for the "reusable widget" goal. Textual already provides composable widgets, overlays, focus handling, and differential rendering. Not specifying risks building from scratch what already exists.
- **"Delay Rust until performance ceiling" is good, but what's the trigger?** Without a defined trigger, this becomes "never." Specify: "Rust extraction is justified when Python rendering latency exceeds X ms or CPU usage exceeds Y% during normal operation."

### Recommendations

1. Define the first concrete deliverable: "A terminal status panel showing active route, lease state, and last 10 task events, rendered with Textual."
2. Name the Python TUI library. Textual is the obvious choice given the requirements.
3. Define the Rust trigger metric.

---

## Section 4: HeiwaPods

### Strengths

- "Build as a control-plane abstraction over existing worker registration, not a separate system" — correct. Workers already register with capabilities via WebSocket. Pods layer metadata on top.
- The pod record schema (provider/host identity, capabilities, trust tier, GPU inventory, model-serving capacity, liveness, leaseability) covers the real dimensions.
- "The first implementation target is not 'rent arbitrary cloud GPUs instantly'" — good calibration. Phase 1 normalizes existing workers, which is immediately useful.
- Three-phase staging (normalize workers → remote GPU metadata → provider-specific lifecycle) is realistic.
- Keeping provider bootstrapping out of the core control plane is architecturally clean.

### Concerns

- **Phase 1 value proposition is thin.** Normalizing existing MacBook/WSL workers as "pods" when there are only 2-3 of them adds ceremony without clear benefit. The value only appears in Phase 2+. Consider whether Phase 1 should instead focus on making the pod schema the canonical worker record (replacing the current ad-hoc capabilities dict) — that's a concrete improvement with a smaller blast radius.
- **Trust/privacy tier integration with routing.** The plan mentions trust tier in the pod record but doesn't connect it to the existing `privacy_level` in routing contracts. How does a pod's trust tier interact with a task's privacy level? This is the interesting part and it's handwaved.
- **GPU inventory schema.** "GPU inventory and model-serving capacity" is listed but not defined. Even a rough schema (gpu_type, vram_gb, loaded_models, available_slots) would help. Without it, Phase 2 implementations will diverge.

### Recommendations

1. Reframe Phase 1 as: "Replace the ad-hoc capabilities dict in WorkerSessionManager with the pod record schema. All existing worker registration flows produce pod records."
2. Explicitly connect pod trust tier to routing privacy level: "Tasks with privacy_level=sovereign only route to pods with trust_tier=local."
3. Sketch a minimal GPU inventory schema for Phase 2.

---

## Public Interfaces / Contracts

### Assessment

The five contracts listed are the right ones. They map directly to the four sections. One gap:

- **Missing: hook registration contract.** How do hooks get registered? Are they hardcoded in OpenClaw/ToolMesh? Configurable per-agent? Loaded from STDB? The plan specifies hook _behavior_ but not hook _registration_.

---

## Test Plan

### Assessment

The test scenarios cover the critical paths. Specific feedback:

- **Route and gateway tests** — Good coverage. Add: "execution proceeds when a valid lease exists and scopes match" (happy path).
- **STDB/control-plane tests** — Good. Add: "pod registration produces a valid pod record with all required fields."
- **Operator UI tests** — "degraded/offline hub behavior still renders sensibly" is a good edge case to include early.
- **Acceptance scenarios** — These read more like integration/E2E scenarios. That's fine, but call them that. Also add: "operator can revoke a lease and see active execution fail-closed."

### Missing test category

- **Hook contract tests**: "Custom hook can deny execution and the denial propagates to the caller." "Custom hook failure is fail-closed." "after_tool_call hook can append metadata without breaking result flow."

---

## Assumptions

### Assessment

All five assumptions are valid and match the current architecture. One addition:

- **Missing assumption: STDB CLI bridge is sufficient for hook-path latency.** The lease lookup in the execution hot path goes through subprocess calls. If this assumption breaks, the architecture needs a lease cache. State it explicitly so it can be monitored.

---

## Cross-Cutting Observations

### What the plan gets right

1. **Python-first.** Resists the temptation to rewrite in Rust prematurely.
2. **Extend, don't replace.** Every proposal builds on existing infrastructure (BaseAgent, OpenClaw, STDB, worker registration).
3. **Copy mechanics, not identity.** Takes pi-mono's evented execution and hygiene without becoming a coding-agent toolkit.
4. **Phased approach.** Each section can be implemented independently, and HeiwaPods is explicitly staged.

### What's missing from the plan

1. **Prioritization across sections.** Sections 1-4 are ordered, but are they sequential or parallel? Can Section 3 (UI) start before Section 2 (hooks/leases) is done? The dependencies should be explicit:
   - Section 1 (standards) is independent — start anytime.
   - Section 2 (hooks/leases) is the foundation for Section 4 (pods need lease-aware routing).
   - Section 3 (UI) depends on Section 2 for state to render, but widget infrastructure can start early.
2. **Milestone definitions.** Each section should have a "done when" definition. Currently the plan describes _what_ but not _when it's done_.
3. **Migration path.** Sections 2 and 4 change how execution works. What's the migration path? Big bang switchover to lease-gated execution, or gradual rollout with a feature flag? The plan should specify.
4. **Rollback strategy.** If lease-gated execution breaks production, how do you revert? Feature flags? Runtime config? This is especially important because GitHub Actions and hosted deploy jobs may run after merge.
5. **Observability.** The plan adds hooks and enforcement but doesn't mention how operators observe hook activity. Add a "hook execution log" to the UI subsystem requirements.

---

## Summary Table

| Section                | Feasibility | Foundation Quality             | Gaps                                                  | Risk   |
| ---------------------- | ----------- | ------------------------------ | ----------------------------------------------------- | ------ |
| 1. Agent Standards     | High        | Standards exist but fragmented | Enforcement mechanism, versioning                     | Low    |
| 2. Tool Hooks + Leases | High        | OpenClaw + STDB leases exist   | STDB latency, scope matching semantics, failure modes | Medium |
| 3. Operator UI         | Medium-High | CLI exists, bare-bones         | No library chosen, no first deliverable defined       | Low    |
| 4. HeiwaPods           | Medium      | Worker registration exists     | Phase 1 value thin, trust/routing integration vague   | Low    |

**Overall: Ship it with the amendments above.** The plan is architecturally sound, well-grounded in the existing codebase, and correctly scoped. The main risks are in Section 2 (STDB latency in the hot path, underspecified failure semantics) and should be addressed before implementation starts.
