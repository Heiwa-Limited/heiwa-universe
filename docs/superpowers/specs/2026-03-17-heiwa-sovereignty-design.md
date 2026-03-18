# Sub-project 1: Sovereignty Foundation (State & Security)

## 1. Overview
This is the first phase of the "Heiwa One: Value Squeezer" overhaul. The pipeline is: **Devon -> Heiwa -> Mesh -> Heiwa -> Loop**. For this loop to be fast, secure, and fully aware of the mesh's state, we must establish a rock-solid foundation. This sub-project purges legacy state management (SQLite), migrates critical usage telemetry to the authoritative SpacetimeDB layer, and centralizes the Digital Barrier to prevent token leakage.

## 2. Architecture & Changes

### 2.1 Centralized Security Service (The Shield)
*   **Problem**: `HEIWA_AUTH_TOKEN` is read via `os.getenv` and compared manually in multiple files (`mcp_server.py`, `spine.py`, `transport.py`), creating a risk of logging the raw token or inconsistent validation.
*   **Solution**: Enhance `packages/heiwa_sdk/heiwa_sdk/security.py` with a `SecurityService` class.
*   **Implementation**:
    *   Add `SecurityService.validate_token(token: str) -> bool`.
    *   Add `SecurityService.get_expected_token() -> str` (which strictly reads from `settings.HEIWA_AUTH_TOKEN`).
    *   Refactor `mcp_server.py` (`_validate_auth_token`), `SpineAgent.handle_request`, and `WorkerSessionManager.worker_socket` to use this centralized service.

### 2.2 SQLite Purge (SpacetimeDB Sovereignty)
*   **Problem**: `db.py` contains extensive SQLite fallback logic (`_init_sqlite_schema`, `get_connection`, etc.). Production logs show SQLite migration warnings, indicating the system is still trying to use it despite the STDB directive.
*   **Solution**: Remove all SQLite dependencies. SpacetimeDB is the single source of truth.
*   **Implementation**:
    *   Remove `import sqlite3` from `packages/heiwa_sdk/heiwa_sdk/db.py`.
    *   Delete methods: `_init_sqlite_schema`, `get_connection`, `_exec`, `_row_to_dict`, `_rows_to_dicts`.
    *   Update all CRUD methods (e.g., `record_run`, `create_mission`) to *only* execute if `self.stdb` is truthy. If STDB is missing, log a critical warning and return early (stateless mode).

### 2.3 Telemetry Migration (The Ledger)
*   **Problem**: `TelemetryAgent` stores live rate limits and model usage in an ephemeral `self.usage_cache` dict. If the Railway hub restarts, the "Value Squeezer" loses its routing context.
*   **Solution**: Rely entirely on SpacetimeDB for telemetry.
*   **Implementation**:
    *   Remove `self.usage_cache` from `apps/heiwa_hub/agents/telemetry.py`.
    *   Ensure `TelemetryAgent.handle_exec_result` writes usage directly via `db.record_run`.
    *   The `ComputeRouter` already has logic to query STDB for model tiers; ensure it relies on STDB for live usage stats when making routing decisions, rather than querying the `TelemetryAgent`'s ephemeral cache.

## 3. Success Criteria
1.  **Zero SQLite Warnings**: Running the Hub in production yields zero SQLite migration or connection warnings.
2.  **Stateless Restart**: Restarting the Hub process does not reset the mesh's awareness of model token usage.
3.  **Centralized Auth**: Grepping for `HEIWA_AUTH_TOKEN` in the codebase shows it is only accessed via `settings.py` and validated via `security.py`.
