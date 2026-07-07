# Spec: DREX Preflight Evidence Persistence

**Status:** Draft
**Date:** 2026-04-04
**Topic:** DREX Preflight Evidence Persistence

## 1. Problem Statement

DREX now performs preflight checks to intercept deterministic or underspecified prompts before they hit expensive model tiers. However, these decisions are currently transient. To prove Heiwa's value (cost avoidance) and provide a verifiable audit trail, these preflight decisions must be persisted as evidence.

## 2. Proposed Changes

### 2.1. SpacetimeDB Schema Updates (`apps/heiwa_hub/spacetimedb/src/lib.rs`)

- Add `DrexPreflightRow` table to track preflight outcomes.
- Add `record_drex_preflight` reducer.

### 2.2. Core Logic Updates (`apps/heiwa_core/src/drex/router.rs`)

- Update `PreflightDecision` to include a unique `decision_id`.
- Ensure `preflight_execution` generates this ID.

### 2.3. Shell Integration (`apps/heiwa_shell/src/main.rs`)

- Wire `preflight_execution` results to the persistence layer (via a background task or immediate call if STDB is connected).

## 3. Data Model

### `DrexPreflightRow`

| Field            | Type             | Description                                                   |
| ---------------- | ---------------- | ------------------------------------------------------------- |
| `preflight_id`   | `String` (PK)    | Unique ID for this preflight check                            |
| `request_id`     | `String` (Index) | Associated request/task ID                                    |
| `execution_mode` | `String`         | `DETERMINISTIC`, `CLARIFY`, `LOCAL_MODEL`, `REMOTE_MODEL`     |
| `reason`         | `String`         | Why this mode was chosen (e.g., "greeting", "underspecified") |
| `input_text`     | `String`         | The raw text passed to preflight                              |
| `response_text`  | `Option<String>` | The deterministic response emitted, if any                    |
| `created_at_ms`  | `u64`            | Timestamp                                                     |

## 4. Success Criteria

- Every `hi` or `help` command in `heiwa shell` creates an entry in the preflight evidence table.
- Evidence entries correctly reflect the `execution_mode` and `reason`.
- The system can report on "Tokens Saved" by calculating avoided calls.
