# Plan: Resolve Healthcheck Drift and Harmonize Documentation

## Objective
Normalize healthcheck endpoint references across the repository to use `/health` (the current standard) instead of the legacy `/ready`, and harmonize architecture documentation to reflect the local-first runtime truth.

## Key Files & Context
- `HEIWA.md`: Contains a "Current Repo Drift" section that needs to be removed once resolved.
- `docs/standards/runtime-baseline.md`: Contains a reference to `/ready` that must be updated.
- `README.md`: Needs to ensure it consistently reflects the local-first direction.
- `apps/heiwa_core/src/runtime/mod.rs`: Already confirmed to use `/health`.

## Implementation Steps

### 1. Resolve Path Drift
- [ ] **Modify `docs/standards/runtime-baseline.md`**: Update any remaining `/ready` references to `/health` (one confirmed in "Non-negotiables").

### 2. Harmonize `HEIWA.md`
- [ ] **Remove "Current Repo Drift" section**: Delete the section (Lines 327–351) as the drift is resolved.

### 3. Verify Consistency
- [ ] **Grep Search**: Run `grep -r "/ready" .` to ensure no active code or baseline docs still use the legacy path (excluding pnpm/node_modules).

## Verification & Testing
- [ ] **Documentation Review**: Manually check the updated files for clarity and consistency with the "local-first" mandate.
