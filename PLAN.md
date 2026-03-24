# Heiwa Inspired-by-Pi Roadmap

## Summary

Adopt the strongest `pi-mono` ideas without collapsing Heiwa into a toolkit clone. The roadmap stays Python-first for operator UX, keeps SpacetimeDB as the control-plane authority, and turns today’s partial lease/worker primitives into explicit execution policy. The implementation order is:

1. Consolidate agent standards and execution contracts
2. Add tool lifecycle hooks and real lease enforcement
3. Upgrade the operator surface into a reusable Python-first UI layer using Textual
4. Formalize `HeiwaPods` on top of the same route/lease model

This intentionally copies `pi-mono`’s mechanics, not its product identity: evented agent execution, differential terminal UX ideas, strict agent hygiene, and pod lifecycle management.

## Integration & Rollout Strategy

- **Migration & Rollback**: All execution path changes (Sections 2 and 4) must be feature-flagged initially. Because Railway auto-deploys on main, we use explicitly defined rollout modes:
  - `observe`: log mismatches, do not block (fail-open).
  - `enforce`: deny on mismatch or hook failure (fail-closed).
  Note: `before_tool_call` internal errors are always fail-closed once enforcement mode is enabled. We will run in `observe` mode first and switch to `enforce` mode via runtime config in SpacetimeDB only after sufficient HeiwaBench coverage.
- **Dependencies & Order**: 
  - **Section 1** (Standards) is independent and can start immediately.
  - **Section 2** (Hooks/Leases) provides the foundational state for UI and routing, so it must precede Section 4.
  - **Section 3** (UI) relies on Section 2 for complete state to render, but base widget infrastructure can be built in parallel.
- **Observability**: Hook activity (denials, execution blocking), lease mismatches, and operator approval gating must be explicitly logged to STDB and surfaced in the Operator UI to ensure failures are transparent.

## Key Changes

### 1. Agent standards become a first-class Heiwa contract

- Create one canonical Heiwa agent standard document that replaces the current split across repo/operator docs and encodes:
  - strict typing expectations
  - required verification commands
  - commit/change logging rules
  - rules for tool execution, file mutation, and risky actions
- Add a **version field** to the standard contract to ensure transparency on which standard a task executed against.
- Treat the standard as part of the runtime contract, not just contributor docs, with an explicit enforcement path:
  - loaded as task-start context for Class 3 execution.
  - surfaced in operator UI (displaying the active standard).
  - validated by HeiwaBench (a test suite to guarantee the presence and shape of the loaded standards/config).
- Keep the model routing philosophy and sovereignty rules that are already Heiwa-specific; do not import `pi-mono`’s conventions blindly.
- **Done When**: The canonical, versioned standard document exists, is automatically included in Class 3 execution context, and HeiwaBench blocks runs missing standard validation.

### 2. Introduce explicit tool lifecycle interception and lease-gated execution

- Add pre/post tool execution hooks to Heiwa’s execution path, implemented at Heiwa’s actual choke points:
  - route resolution and dispatch in `OpenClaw`
  - final wrapper invocation in `ToolMesh`
- Standardize a hook context that includes:
  - proposal/task identifiers
  - assigned worker/runtime
  - requested tool name and parsed arguments
  - active lease metadata
  - risk/privacy classification
- `before_tool_call` must be able to:
  - allow
  - deny with a structured reason (Failures within `before_tool_call` itself must be **fail-closed**, resulting in an execution denial).
  - request operator approval when policy requires it
  - append metadata with a narrower shape like `append_audit_metadata` (replacing the riskier `modified_context`).
- `after_tool_call` must be able to:
  - attach audit metadata
  - redact or transform tool output before publication
  - record structured usage/result state into STDB
- Move lease handling from “state exists” to “execution denied without a valid lease”:
  - `OpenClaw` and `ToolMesh` reject execution when there is no active matching lease.
  - **Scope matching semantics**:
    - `tool_scope`: Exact match.
    - `filesystem_scope`: Path prefix match.
    - `network_scope`: Host/domain allowlist match.
    - `secret_scope`: Exact secret ID match.
  - **Performance/Latency Assumption**: STDB lookup latency in the hot path via CLI subprocesses is accepted for v1 (assuming low throughput). A lease cache with subscription-based invalidation is planned when performance dictates.
- Keep `BaseAgent` thin. The authoritative enforcement point should remain in the execution gateway/tool layer.
- **Done When**: SpacetimeDB leases are strictly enforced at execution time, matching correct scopes, failing closed on mismatch or hook error, and fully observable.

### 3. Evolve the current CLI into a reusable operator UI subsystem

- Build the first serious operator UI using the **Textual** Python UI library, capitalizing on existing composable widgets and avoiding an early Rust rewrite.
- The **first concrete deliverable**: A terminal control-plane panel showing the active route, lease status, approval state, and the last 10 task events.
- Reframe the current shell into a package-oriented UI contract:
  - reusable status/footer widgets
  - structured message/tool result renderers
  - overlay/dialog/select primitives
  - route/latency/runtime telemetry panels
- Borrow the useful `pi-tui` ideas conceptually: differential redraw, overlay-based interaction, explicit focus/input handling.
- Treat Discord/web formatting separately from the terminal renderer. The goal is a shared UI data model, not one renderer for every surface.
- Delay any Rust limb extraction until Textual rendering latency exceeds 50ms or steady-state CPU usage exceeds 15% during normal operation.
- **Done When**: The Textual-based terminal dashboard runs locally, connects to the hub, and accurately displays active control-plane state continuously.

### 4. Formalize `HeiwaPods` from today’s boost-node primitives

- Build `HeiwaPods` as a control-plane abstraction over the existing worker registration and routing model, not as a separate system.
- Explicitly connect `trust_tier` to routing privacy: e.g., sovereign tasks *only* route to local-trust pods.
- Define pod records and lifecycle around:
  - provider/host identity
  - runtime capabilities
  - trust/privacy tier
  - GPU inventory and model-serving capacity (minimal schema: `gpu_type`, `vram_gb`, `loaded_models`, `available_slots`)
  - liveness and leaseability
- Add a minimal pod management surface inspired by `pi pods`:
  - register/list/select/deactivate pods
  - inspect capabilities and current allocations
  - attach model-serving metadata
- Keep provider-specific bootstrapping out of the core control plane. Use adapter scripts/services per provider later.
- Process staging:
  - **Phase 1**: Explicitly replace the ad hoc worker capability record with the formalized pod record schema within the existing registration flow.
  - **Phase 2**: Support remote GPU pod metadata and validate trust-tier routing.
  - **Phase 3**: Support provider-specific setup/start/stop flows.
- **Done When**: All connected workers register using the Pod schema, and the router actively enforces `trust_tier` constraints.

## Public Interfaces / Contracts

- **Execution hook contract**:
  - `before_tool_call(context) -> allow | deny | approval_required | append_audit_metadata`
  - `after_tool_call(context) -> audit/result metadata updates`
- **Hook registration contract**:
  - Hook implementations are defined in repo code, but the authoritative mapping of which hooks run for a given route/tool is loaded dynamically from runtime config in SpacetimeDB.
- **Lease validation contract**:
  - Every tool execution requires an active lease matching tool/runtime/scope.
- **UI state contract**:
  - Structured operator event/view model for route status, tool execution, approval state, and lease state.
- **Pod contract**:
  - Persistent pod metadata with capability, trust tier, liveness, allocation state, and the minimal GPU inventory schema.
- **Standards contract**:
  - One canonical Heiwa agent-standard source (versioned) consumed by operator/runtime surfaces and release gates.

## Test Plan

- **Route and gateway tests**:
  - execution proceeds when a valid lease exists and scopes match.
  - execution is denied when no active lease exists or tool scope mismatch occurs.
  - custom hook can deny execution and the denial propagates back.
  - `before_tool_call` failures fail-closed (deny execution).
  - `after_tool_call` can append metadata without breaking result flow.
- **STDB/control-plane tests**:
  - lease issuance, renewal, and revocation remain compatible with current proposal flow.
  - pod registration produces a valid pod record with all required fields map cleanly.
- **Operator UI tests**:
  - terminal renderers handle route/tool/approval/lease state updates without breaking interaction.
  - degraded/offline hub behavior still renders sensibly.
- **Acceptance scenarios**:
  - local sovereign file mutation routes to a trusted local pod and shows lease/runtime state.
  - high-risk operation requests approval before execution.
  - operator can revoke a lease and see active execution fail-closed immediately.
  - operator can see route, tool, runtime, lease, and failure reason from the terminal surface.

## Assumptions

- The roadmap covers all four inspiration areas in one phased program, not a single-feature implementation.
- UI remains Python-first (Textual) for the first iteration; no early Rust rewrite.
- STDB CLI bridge latency is sufficient for lease lookups in the hook path v1, pending an eventual lease cache.
- SpacetimeDB remains the authoritative state layer.
- Existing worker WebSocket registration and lease tables are retained and extended, not replaced.
- Heiwa should copy `pi-mono`’s eventing, hygiene, and UX patterns where useful, but keep Heiwa’s OS/control-plane identity rather than converging on a generic coding-agent toolkit.
