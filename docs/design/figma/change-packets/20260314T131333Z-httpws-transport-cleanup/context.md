# Heiwa Figma Sync Packet

Timestamp: 2026-03-14T13:13:33Z
Scope: transport cleanup + operator ingress hardening

## What Changed

- Removed the remaining active NATS bootstrap/runtime residue from worker and ops scripts.
- Normalized local worker boot around hub HTTP/WebSocket ingress plus optional local Ollama/OpenClaw.
- Replaced the stale `runtime/fleets/hub/start.sh` implementation with a compatibility wrapper that delegates to the canonical hub entrypoint.
- Updated the March 6 blueprint and hub identity/config surfaces so the repo describes the current architecture: HTTP/WebSocket ingress, in-process local bus, websocket workers.
- Added an end-to-end task ingress test covering `POST /tasks -> Spine -> Executor -> /ws/tasks/{task_id}`.

## Visual/Product Implication

- Heiwa should now be diagrammed as one ingress surface with three transport layers:
  - operator surface: CLI / app composer
  - hub ingress: authenticated HTTP + task websocket streams
  - internal dispatch: local bus + websocket workers
- Remove any Figma topology that still shows NATS as the primary cloud control-plane bus.

## Files Of Interest

- `/Users/dmcgregsauce/heiwa/apps/heiwa_cli/scripts/ops/start_worker_stack.sh`
- `/Users/dmcgregsauce/heiwa/apps/heiwa_cli/scripts/ops/worker_manager_daemon.sh`
- `/Users/dmcgregsauce/heiwa/runtime/fleets/hub/start.sh`
- `/Users/dmcgregsauce/heiwa/config/swarm/BUILD_BLUEPRINT_2026-03-06.md`
- `/Users/dmcgregsauce/heiwa/apps/heiwa_hub/tests/test_task_ingress_e2e.py`

## Human Check

- Update architecture boards so the hub shows HTTP/WebSocket ingress and local bus internals.
- Remove NATS labels from operator-facing diagrams unless they are explicitly historical/archive views.
