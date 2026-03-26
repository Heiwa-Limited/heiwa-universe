# Execution Room

Load this room for:

- worker node execution
- claim / run / result loops
- tool execution boundaries
- capability and lease enforcement

## Physical Execution Surfaces

- MacBook: high-trust orchestrator, local-first operator node
- WSL/Ubuntu: worker node, development and heavier execution lane
- Railway: control-plane host, not the general-purpose worker target

## Live Runtime Path

- Broker decides route
- `HeiwaClaw` resolves execution adapter and transport
- `ToolMesh` executes the selected adapter/tool
- results are written back through the state layer

Relevant files:

- `packages/heiwa_sdk/heiwa_sdk/heiwaclaw.py`
- `packages/heiwa_sdk/heiwa_sdk/tool_mesh.py`
- `apps/heiwa_cli/scripts/agents/worker_manager.py`

## Capability Reality Today

- Worker capability matching is currently node-oriented.
- `worker_manager.py` derives a node capability set from environment or node type.
- proposal targeting logic currently checks `requires` plus `privilege_tier` against node metadata.

This means the current execution boundary is closer to:

- per-proposal
- assigned to a node
- filtered by capability-class and privilege tier

It is not yet true per-tool lease enforcement.

## Direction

- Every meaningful execution should run under a lease.
- `HeiwaClaw` / `ToolMesh` should reject execution without a valid lease.
- Deny-first should apply to both external provider calls and internal cell-to-cell/tool execution.
- Railway should authorize and route. Nodes should execute.
