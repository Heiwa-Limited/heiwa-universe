# Sub-project 3: The Fluid Mesh (Orchestration & Delivery)

## 1. Overview

This phase implements the "Director" and "Router" of the Heiwa mesh. It centralizes the intelligence needed to decompose intents and dispatch tasks while unifying the delivery mechanism across local and remote nodes. This enables any agent (Claude, Gemini, etc.) to act as a project manager, spawning sub-tasks for any other agent in the swarm through a single unified API.

## 2. Architecture & Changes

### 2.1 Orchestration Service (The Director)

- **Goal**: Provide a high-level SDK interface for task lifecycle management.
- **Implementation**: Create `packages/heiwa_sdk/heiwa_sdk/orchestration.py`.
  - Integrate `IntentNormalizer`, `RiskScorer`, and `ComputeRouter` into a single `orchestrate(raw_text)` pipeline.
  - Add `spawn_subagent(task_id, instruction, target_worker)` to create child tasks in the SpacetimeDB missions table.
- **Refactor**: Deprecate `BrokerEnrichmentService` in `heiwa_cognition` and migrate its logic here.

### 2.2 Delivery Manager (The Router)

- **Goal**: Unify LocalBus and WebSocket task delivery.
- **Implementation**: Create `apps/heiwa_hub/delivery.py` with a `DeliveryManager` class.
  - `deliver(route: BrokerRouteResult)`:
    1. Validates the target `assigned_worker`.
    2. If the worker is "local" (the Hub node), publishes to `Subject.TASK_EXEC` on the `LocalBus`.
    3. If the worker is "remote" (MacBook, WSL), calls `WorkerSessionManager.push_task`.
- **Refactor**: Update `SpineAgent` to call `DeliveryManager.deliver()` after enrichment.

### 2.3 Feedback Loops (The Harness)

- **Goal**: Close the loop between execution and reasoning with actionable logs.
- **Implementation**:
  - Standardize the `TASK_EXEC_RESULT` payload to include an `artifacts` dict.
  - Update `ExecutorAgent` to capture the last 50 lines of stdout/stderr and include them in the result artifacts.
  - Ensure the `DeliveryManager` routes these results back to the `parent_task_id` if it was a sub-task.

## 3. Success Criteria

1. **Unified Ingress**: Both `mcp_server.py` and `SpineAgent` use `OrchestrationService.orchestrate()`.
2. **Omnidirectional Dispatch**: A test script can spawn a task on the Hub that is automatically delivered to a remote WebSocket worker without manual routing logic.
3. **Harness Awareness**: Agent results in Discord include a "Diagnostic Logs" section if the task failed.
