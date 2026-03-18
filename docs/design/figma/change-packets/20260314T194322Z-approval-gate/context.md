# Heiwa Approval Gate Sync Packet

## Change Summary

Phase A approval gating is now implemented in the live hub runtime.

- Spine enforces `requires_approval` after planning and before dispatch.
- Approval policy is surface-aware:
  - low risk auto-approves everywhere
  - medium risk auto-approves on CLI and web/api
  - high risk auto-approves on CLI only
  - critical risk always holds
- `HEIWA_AUTO_APPROVE=all` bypasses the gate for development.
- Hub endpoints now support:
  - `GET /approvals`
  - `POST /tasks/{id}/approve`
  - `POST /tasks/{id}/reject`
- CLI now supports:
  - `heiwa approve <task_id>`
  - `heiwa reject <task_id>`
- Task snapshots and websocket clients now surface `AWAITING_APPROVAL`, `APPROVED`, `REJECTED`, and `EXPIRED`.

## Product Implication

The operator surface is no longer pretending. High-risk and critical work can be halted and resumed explicitly. This is the foundation for the app right rail approvals queue.

## UI Follow-Through

The app should add an approvals panel that consumes authenticated approval state and renders:

- task excerpt
- risk level
- source surface
- expires-at / countdown
- approve / reject controls

## Data Contracts To Mirror In Figma

- Task timeline states now include:
  - `ACKNOWLEDGED`
  - `AWAITING_APPROVAL`
  - `APPROVED`
  - `REJECTED`
  - `EXPIRED`
  - `DISPATCHED_PLAN`
  - `RUNNING`
  - `PASS`
  - `FAIL`
- Approval queue item fields:
  - `task_id`
  - `approval_id`
  - `status`
  - `risk_level`
  - `source_surface`
  - `requested_by`
  - `raw_text_excerpt`
  - `created_at`
  - `expires_at`

## Remaining Next Step

Phase B: multi-step decomposition and active task progression.
