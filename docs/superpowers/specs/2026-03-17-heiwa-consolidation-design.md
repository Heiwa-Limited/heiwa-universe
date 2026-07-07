# Sub-project 2: Workspace Consolidation (Identity)

## 1. Overview

Heiwa is defined as "One Logical Identity". Currently, this identity is fragmented across multiple "ghost" repositories in the home directory (`heiwa-core`, `heiwa-spacetime`, `heiwa-limited-repo`). This sub-project harvests the unique logic and history from these fragments, integrates them into the primary `~/heiwa` monorepo, and archives the redundant structures to enforce physical sovereignty.

## 2. Architecture & Changes

### 2.1 Component Harvest (The Merge)

- **Design History**: Migrate the Figma change packet history.
  - Target: `~/heiwa/docs/design/figma/`
  - Source: `~/heiwa-core/figma/`
- **Rust Limbs**: Integrate the Rust-based SpacetimeDB connection experiments.
  - Target: `~/heiwa/apps/heiwa_limbs/rust_limb/`
  - Source: `~/heiwa-spacetime/apps/iclaw-rust-limb/`
- **Audit Reports**: Integrate system integrity logs.
  - Target: `~/heiwa/docs/audit/signals_integrity_report.md`
  - Source: `~/heiwa-limited-repo/signals_integrity_report.md`

### 2.2 Physical Archival (The Cleanup)

- **Objective**: Remove clutter from the `~` workspace while preserving data.
- **Action**: Create a timestamped archive directory `~/heiwa_archive/consolidation-2026-03-17/`.
- **Moves**: Move `heiwa-core`, `heiwa-spacetime`, and `heiwa-limited-repo` into the archive.

### 2.3 Documentation Alignment (The Truth)

- **Objective**: Update internal READMEs and architectural docs to reflect the new state.
- **Changes**:
  - **HEIWA.md**: Explicitly label all other repos as "Legacy/Archived".
  - **AGENTS.md**: Update to reflect the integration of `heiwa_limbs` as the official way to connect non-Python components to the mesh.
  - **CLAUDE.md / GEMINI.md**: Ensure they point to the consolidated `~/heiwa` as the only workspace.

## 3. Success Criteria

1. **Single Active Repo**: `ls -ld ~/heiwa*` returns only `~/heiwa` and `~/heiwa_archive`.
2. **Logic Preservation**: The Rust limb in `apps/heiwa_limbs/rust_limb/` is buildable via `cargo build`.
3. **Doc Accuracy**: A search for "heiwa-core" in the `~/heiwa` docs returns only archival notes.
