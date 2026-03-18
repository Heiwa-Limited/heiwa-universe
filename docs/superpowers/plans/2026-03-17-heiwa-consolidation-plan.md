# Workspace Consolidation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Physically unify the Heiwa identity by harvesting unique logic and history from ghost repositories and archiving redundant structures.

**Architecture:** 
1. **Harvest & Merge**: Surgical `cp -r` and `mv` operations to move Figma change packets, Rust STDB limbs, and system integrity reports into the `~/heiwa` monorepo.
2. **Clean Archival**: Move all other Heiwa-related root directories into a timestamped subfolder within `~/heiwa_archive`.
3. **Doc Alignment**: Update primary documentation (`HEIWA.md`, `AGENTS.md`) to reflect the consolidated state.

**Tech Stack:** Bash (mv, cp, mkdir, ls), Git.

---

## Chunk 1: Component Harvest & Merge

**Files:**
- Create: `docs/design/figma/`
- Create: `apps/heiwa_limbs/rust_limb/`
- Create: `docs/audit/signals_integrity_report.md`

### Task 1: Migrate Figma Design History

- [ ] **Step 1: Create target directory**
Run: `mkdir -p ~/heiwa/docs/design/figma`
- [ ] **Step 2: Copy history from heiwa-core**
Run: `cp -r ~/heiwa-core/figma/change-packets ~/heiwa/docs/design/figma/`
- [ ] **Step 3: Verify copy**
Run: `ls ~/heiwa/docs/design/figma/change-packets`
Expected: List of timestamped folders.
- [ ] **Step 4: Commit**
```bash
git add docs/design/figma/
git commit -m "feat(docs): integrate figma change packet history from legacy core"
```

### Task 2: Migrate Rust STDB Limbs

- [ ] **Step 1: Create target directory**
Run: `mkdir -p ~/heiwa/apps/heiwa_limbs/rust_limb`
- [ ] **Step 2: Copy source code from heiwa-spacetime**
Run: `cp -r ~/heiwa-spacetime/apps/iclaw-rust-limb/* ~/heiwa/apps/heiwa_limbs/rust_limb/`
- [ ] **Step 3: Verify build capability**
Run: `cd ~/heiwa/apps/heiwa_limbs/rust_limb && cargo check`
Expected: Successful check (or missing deps, but files must exist).
- [ ] **Step 4: Commit**
```bash
git add apps/heiwa_limbs/rust_limb/
git commit -m "feat(limbs): integrate rust-based stdb limb experiments"
```

### Task 3: Migrate Integrity Reports

- [ ] **Step 1: Create target directory**
Run: `mkdir -p ~/heiwa/docs/audit`
- [ ] **Step 2: Move report from heiwa-limited-repo**
Run: `cp ~/heiwa-limited-repo/signals_integrity_report.md ~/heiwa/docs/audit/`
- [ ] **Step 3: Commit**
```bash
git add docs/audit/
git commit -m "docs(audit): integrate system integrity reports"
```

## Chunk 2: Workspace Cleanup & Archival

**Files:**
- Create: `~/heiwa_archive/consolidation-2026-03-17/`

### Task 4: Archive Ghost Repos

- [ ] **Step 1: Create timestamped archive**
Run: `mkdir -p ~/heiwa_archive/consolidation-2026-03-17`
- [ ] **Step 2: Move ghost repos to archive**
Run: `mv ~/heiwa-core ~/heiwa-spacetime ~/heiwa-limited-repo ~/heiwa_archive/consolidation-2026-03-17/`
- [ ] **Step 3: Verify workspace state**
Run: `ls -ld ~/heiwa*`
Expected: Only `~/heiwa` and `~/heiwa_archive`.

## Chunk 3: Documentation Alignment

**Files:**
- Modify: `HEIWA.md`
- Modify: `AGENTS.md`

### Task 5: Update Internal Truth

- [ ] **Step 1: Update HEIWA.md**
Add a "Legacy/Archived Repositories" section at the end of `HEIWA.md` explicitly stating that `heiwa-core`, `heiwa-spacetime`, and `heiwa-limited` have been consolidated into the monorepo as of 2026-03-17.
- [ ] **Step 2: Update AGENTS.md**
Update the architecture section to include `apps/heiwa_limbs/` as the standard location for cross-language mesh components.
- [ ] **Step 3: Final Commit**
```bash
git add HEIWA.md AGENTS.md
git commit -m "docs: align architectural truth with consolidated workspace state"
```
