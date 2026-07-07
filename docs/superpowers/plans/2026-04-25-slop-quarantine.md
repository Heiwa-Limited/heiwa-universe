# Slop Quarantine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move every path classified `legacy` or `archive` (per `PRODUCT_SURFACE.md`) under `legacy/` and `archive/` subtrees so the product center of the repo (`apps/`, `crates/`, top-level `packages/`) contains only product surfaces. No deletions in this plan — quarantine only.

**Architecture:** Use `git mv` to preserve history. Update `PRODUCT_SURFACE.md` path references in the same commit as each move. Verify each move with the audit script — class totals must remain identical (only path strings change). Cargo/Python workspace members are updated to point at new paths so the workspace continues to build.

**Tech Stack:** Git, bash. No new code. Workspace manifests touched: `Cargo.toml`, `pyproject.toml`, `package.json`, `mkdocs.yml`.

**Prerequisite:** Plan 1 (`2026-04-25-product-surface-definition.md`) merged. `PRODUCT_SURFACE.md` exists and `scripts/audit_product_surface.sh` runs.

---

## Why move and not delete

Per `HEIWA.md` non-negotiable: "honesty over completeness theater." Deletion needs a soak period of "no traffic" evidence. Quarantine achieves the boundary effect without losing optionality. Movement under `legacy/` is also a strong social signal — contributors stop accidentally extending these surfaces.

## File Structure

| Source path                | Destination                       | Class   |
| -------------------------- | --------------------------------- | ------- |
| `apps/heiwa_hub`           | `legacy/apps/heiwa_hub`           | legacy  |
| `apps/heiwa_cli`           | `legacy/apps/heiwa_cli`           | legacy  |
| `apps/heiwa_limbs`         | `legacy/apps/heiwa_limbs`         | legacy  |
| `apps/heiwa_dj`            | `archive/apps/heiwa_dj`           | archive |
| `packages/heiwa_skills`    | `legacy/packages/heiwa_skills`    | legacy  |
| `packages/heiwa_cognition` | `legacy/packages/heiwa_cognition` | legacy  |
| `packages/heiwa_ui`        | `legacy/packages/heiwa_ui`        | legacy  |
| `node`                     | `legacy/node`                     | legacy  |

8 moves. Each is one task. After each move:

1. Update workspace manifests
2. Update `PRODUCT_SURFACE.md`
3. Verify build
4. Run audit
5. Commit

---

### Task 0: Pre-flight verification

- [ ] **Step 1: Confirm Plan 1 has landed**

Run: `test -f PRODUCT_SURFACE.md && test -x scripts/audit_product_surface.sh && echo OK`
Expected: `OK`

- [ ] **Step 2: Capture baseline audit**

Run: `./scripts/audit_product_surface.sh > /tmp/baseline_audit.txt && cat /tmp/baseline_audit.txt`
Expected: Per-class LOC report. Save this — every later step compares against it.

- [ ] **Step 3: Confirm clean working tree**

Run: `git status`
Expected: `nothing to commit, working tree clean`

- [ ] **Step 4: Create the destination directories (empty for now)**

Run: `mkdir -p legacy/apps legacy/packages archive/apps && touch legacy/.gitkeep archive/.gitkeep`
Expected: No output. Dirs and gitkeeps exist.

- [ ] **Step 5: Commit the empty destination structure**

```bash
git add legacy/ archive/
git commit -m "chore: add legacy/ and archive/ destination subtrees"
```

---

### Task 1: Quarantine `apps/heiwa_hub` (largest, 24,793 LOC)

**Files:**

- Move: `apps/heiwa_hub/` → `legacy/apps/heiwa_hub/`
- Modify: `Cargo.toml` (workspace members)
- Modify: `pyproject.toml` (workspace members if applicable)
- Modify: `PRODUCT_SURFACE.md`

- [ ] **Step 1: Identify workspace references**

Run: `grep -rn "apps/heiwa_hub" Cargo.toml pyproject.toml package.json mkdocs.yml 2>/dev/null`
Expected: A list of references. Note them — each must be updated.

- [ ] **Step 2: Perform the move**

Run: `git mv apps/heiwa_hub legacy/apps/heiwa_hub`
Expected: No output. `git status` shows the rename.

- [ ] **Step 3: Update Cargo workspace members**

Edit `Cargo.toml`. Replace `"apps/heiwa_hub"` with `"legacy/apps/heiwa_hub"` (or, if the legacy crate is no longer needed in the workspace build, remove the line — preferred to keep build times down).

If removing: open `Cargo.toml`, find the `[workspace.members]` array, delete any line containing `apps/heiwa_hub`. The hub remains in the repo, just outside the active build.

- [ ] **Step 4: Update Python workspace if hub is in `pyproject.toml`**

Run: `grep -n "heiwa_hub" pyproject.toml`
If present, edit to either retarget the path or remove the entry depending on whether the hub Python package is still installable.

- [ ] **Step 5: Update PRODUCT_SURFACE.md**

Edit the table — change `| \`apps/heiwa_hub\` | legacy |`to`| \`legacy/apps/heiwa_hub\` | legacy |`.

- [ ] **Step 6: Verify the build still works**

Run: `cargo check --workspace`
Expected: Clean build. If the hub crate was removed from members, no errors. If retained at new path, references must resolve.

- [ ] **Step 7: Run the audit and compare**

Run: `./scripts/audit_product_surface.sh > /tmp/after_hub.txt && diff /tmp/baseline_audit.txt /tmp/after_hub.txt`
Expected: No diff (LOC totals unchanged — only paths moved within the same class).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: quarantine apps/heiwa_hub under legacy/"
```

---

### Task 2: Quarantine `packages/heiwa_skills` (largest single subtree, 86,477 LOC)

**Files:**

- Move: `packages/heiwa_skills/` → `legacy/packages/heiwa_skills/`
- Modify: `Cargo.toml`, `pyproject.toml`, `package.json` (any references)
- Modify: `PRODUCT_SURFACE.md`

- [ ] **Step 1: Identify references**

Run: `grep -rn "packages/heiwa_skills\|heiwa_skills" Cargo.toml pyproject.toml package.json mkdocs.yml 2>/dev/null | head -30`
Expected: Reference list. Note them.

- [ ] **Step 2: Perform the move**

Run: `git mv packages/heiwa_skills legacy/packages/heiwa_skills`
Expected: No output.

- [ ] **Step 3: Update workspace manifests**

For each match from Step 1, either retarget the path or remove the entry. Preference: remove from active workspace (these are legacy).

- [ ] **Step 4: Update PRODUCT_SURFACE.md**

Replace `| \`packages/heiwa_skills\` | legacy |`with`| \`legacy/packages/heiwa_skills\` | legacy |`.

- [ ] **Step 5: Verify build**

Run: `cargo check --workspace && python -c "import sys; print(sys.version)" 2>/dev/null || true`
Expected: Cargo passes. Python check is informational.

- [ ] **Step 6: Run audit and diff**

Run: `./scripts/audit_product_surface.sh > /tmp/after_skills.txt && diff /tmp/after_hub.txt /tmp/after_skills.txt`
Expected: No diff in totals.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: quarantine packages/heiwa_skills under legacy/"
```

---

### Task 3: Quarantine `apps/heiwa_cli` (3,512 LOC, Python shim)

**Files:**

- Move: `apps/heiwa_cli/` → `legacy/apps/heiwa_cli/`
- Modify: workspace manifests, `PRODUCT_SURFACE.md`

- [ ] **Step 1: Identify references**

Run: `grep -rn "apps/heiwa_cli" Cargo.toml pyproject.toml package.json mkdocs.yml 2>/dev/null`

- [ ] **Step 2: Move**

Run: `git mv apps/heiwa_cli legacy/apps/heiwa_cli`

- [ ] **Step 3: Update manifests and PRODUCT_SURFACE.md**

Replace path references. Update the surface table entry.

- [ ] **Step 4: Verify build**

Run: `cargo check --workspace`
Expected: Clean.

- [ ] **Step 5: Audit + diff**

Run: `./scripts/audit_product_surface.sh > /tmp/after_cli.txt && diff /tmp/after_skills.txt /tmp/after_cli.txt`
Expected: No diff.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: quarantine apps/heiwa_cli (Python shim) under legacy/"
```

---

### Task 4: Quarantine `apps/heiwa_limbs` (2,446 LOC, experimental)

**Files:**

- Move: `apps/heiwa_limbs/` → `legacy/apps/heiwa_limbs/`
- Modify: workspace manifests, `PRODUCT_SURFACE.md`

- [ ] **Step 1: Identify references**

Run: `grep -rn "apps/heiwa_limbs\|heiwa_limbs\|rust_limb" Cargo.toml pyproject.toml package.json 2>/dev/null`

- [ ] **Step 2: Move**

Run: `git mv apps/heiwa_limbs legacy/apps/heiwa_limbs`

- [ ] **Step 3: Update manifests and PRODUCT_SURFACE.md**

- [ ] **Step 4: Verify build**

Run: `cargo check --workspace`
Expected: Clean.

- [ ] **Step 5: Audit + diff**

Run: `./scripts/audit_product_surface.sh > /tmp/after_limbs.txt && diff /tmp/after_cli.txt /tmp/after_limbs.txt`
Expected: No diff.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: quarantine apps/heiwa_limbs under legacy/"
```

---

### Task 5: Quarantine `apps/heiwa_dj` (17 LOC stub → archive)

**Files:**

- Move: `apps/heiwa_dj/` → `archive/apps/heiwa_dj/`
- Modify: `PRODUCT_SURFACE.md`

- [ ] **Step 1: Move**

Run: `git mv apps/heiwa_dj archive/apps/heiwa_dj`

- [ ] **Step 2: Update PRODUCT_SURFACE.md**

Change `| \`apps/heiwa_dj\` | archive |`to`| \`archive/apps/heiwa_dj\` | archive |`.

- [ ] **Step 3: Verify build**

Run: `cargo check --workspace`
Expected: Clean (this was a stub).

- [ ] **Step 4: Audit + diff**

Run: `./scripts/audit_product_surface.sh > /tmp/after_dj.txt && diff /tmp/after_limbs.txt /tmp/after_dj.txt`
Expected: No diff.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: archive apps/heiwa_dj stub"
```

---

### Task 6: Quarantine `packages/heiwa_cognition` (3,031 LOC)

**Files:**

- Move: `packages/heiwa_cognition/` → `legacy/packages/heiwa_cognition/`
- Modify: workspace manifests, `PRODUCT_SURFACE.md`

- [ ] **Step 1: Identify references**

Run: `grep -rn "packages/heiwa_cognition\|heiwa_cognition" Cargo.toml pyproject.toml package.json 2>/dev/null`

- [ ] **Step 2: Move**

Run: `git mv packages/heiwa_cognition legacy/packages/heiwa_cognition`

- [ ] **Step 3: Update manifests and PRODUCT_SURFACE.md**

- [ ] **Step 4: Verify build**

Run: `cargo check --workspace`
Expected: Clean.

- [ ] **Step 5: Audit + diff**

Run: `./scripts/audit_product_surface.sh > /tmp/after_cognition.txt && diff /tmp/after_dj.txt /tmp/after_cognition.txt`
Expected: No diff.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: quarantine packages/heiwa_cognition under legacy/"
```

---

### Task 7: Quarantine `packages/heiwa_ui` (306 LOC)

**Files:**

- Move: `packages/heiwa_ui/` → `legacy/packages/heiwa_ui/`
- Modify: workspace manifests, `PRODUCT_SURFACE.md`

- [ ] **Step 1: Identify references**

Run: `grep -rn "packages/heiwa_ui\|heiwa_ui" Cargo.toml pyproject.toml package.json 2>/dev/null`

- [ ] **Step 2: Move**

Run: `git mv packages/heiwa_ui legacy/packages/heiwa_ui`

- [ ] **Step 3: Update manifests and PRODUCT_SURFACE.md**

- [ ] **Step 4: Verify build**

Run: `cargo check --workspace && (cd apps/heiwa_app && npm install 2>&1 | tail -5)`
Expected: Clean.

- [ ] **Step 5: Audit + diff**

Run: `./scripts/audit_product_surface.sh > /tmp/after_ui.txt && diff /tmp/after_cognition.txt /tmp/after_ui.txt`
Expected: No diff.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: quarantine packages/heiwa_ui under legacy/"
```

---

### Task 8: Quarantine `node/` (legacy node helpers)

**Files:**

- Move: `node/` → `legacy/node/`
- Modify: `PRODUCT_SURFACE.md`

- [ ] **Step 1: Confirm `node/` is not `node_modules`**

Run: `ls node/ && head -1 node/package.json 2>/dev/null || echo 'no package.json'`
Expected: A list of files. If this turns out to be auto-generated dependency tree, abort and reclassify as `runtime-artifact`.

- [ ] **Step 2: Identify references**

Run: `grep -rn '"node":\|"./node"\|/node/' package.json mkdocs.yml 2>/dev/null`

- [ ] **Step 3: Move**

Run: `git mv node legacy/node`

- [ ] **Step 4: Update manifests and PRODUCT_SURFACE.md**

- [ ] **Step 5: Verify build**

Run: `cargo check --workspace`
Expected: Clean (Rust unaffected).

- [ ] **Step 6: Audit + diff**

Run: `./scripts/audit_product_surface.sh > /tmp/after_node.txt && diff /tmp/after_ui.txt /tmp/after_node.txt`
Expected: No diff.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: quarantine top-level node/ helpers under legacy/"
```

---

### Task 9: Add legacy README signposts

**Files:**

- Create: `legacy/README.md`
- Create: `archive/README.md`

- [ ] **Step 1: Write `legacy/README.md`**

```markdown
# Legacy

Surfaces here are kept for migration reference and historical context. They are **not** part of the active `heiwa` product contract.

Do not extend code under this tree. If functionality here is needed, port it into the active product surface (under `crates/`, `apps/heiwa_shell`, etc.) and remove the legacy copy in a follow-up.

Class membership: every path here is `legacy` per `PRODUCT_SURFACE.md`.

Deletion timeline: a path becomes a deletion candidate after one full release cycle of "no traffic" — no imports from product code, no operator references, no documentation links.
```

- [ ] **Step 2: Write `archive/README.md`**

```markdown
# Archive

Frozen snapshots of removed work. **Read-only by convention.**

Files here are not built, not tested, not packaged. They exist for historical reference only.

If you find yourself wanting to extend an archived surface, copy what you need into product code; do not modify in place.
```

- [ ] **Step 3: Commit**

```bash
git add legacy/README.md archive/README.md
git commit -m "docs: add README signposts for legacy/ and archive/ trees"
```

---

### Task 10: Final verification

- [ ] **Step 1: Run full audit**

Run: `./scripts/audit_product_surface.sh`
Expected: `unclassified` is 0. `legacy` and `archive` totals match Task 0 baseline within ±2 (README files added).

- [ ] **Step 2: Confirm workspace builds**

Run: `cargo check --workspace 2>&1 | tail -10`
Expected: `Finished` line, no errors.

- [ ] **Step 3: Confirm no broken doc links**

Run: `grep -rn "apps/heiwa_hub\|apps/heiwa_cli\|apps/heiwa_limbs\|packages/heiwa_skills\|packages/heiwa_cognition\|packages/heiwa_ui" docs/ HEIWA.md README.md 2>/dev/null | grep -v "legacy/"`
Expected: Empty output. If matches appear, those references need updating to `legacy/...` paths.

- [ ] **Step 4: Push branch and open PR**

```bash
git push -u origin HEAD
gh pr create --title "refactor: quarantine legacy and archive surfaces" --body "$(cat <<'EOF'
## Summary
Quarantines all `legacy` and `archive` paths from `PRODUCT_SURFACE.md` under `legacy/` and `archive/` subtrees. No deletions; history preserved via `git mv`.

Moves:
- `apps/heiwa_hub` → `legacy/apps/heiwa_hub` (24,793 LOC)
- `packages/heiwa_skills` → `legacy/packages/heiwa_skills` (86,477 LOC)
- `apps/heiwa_cli`, `apps/heiwa_limbs` → `legacy/apps/`
- `packages/heiwa_cognition`, `packages/heiwa_ui` → `legacy/packages/`
- `apps/heiwa_dj` → `archive/apps/heiwa_dj`
- `node/` → `legacy/node/`

Workspace manifests updated. `cargo check --workspace` passes.

## Test plan
- [x] `./scripts/audit_product_surface.sh` reports zero unclassified
- [x] `cargo check --workspace` passes
- [x] No broken `docs/`, `HEIWA.md`, or `README.md` links to old paths
- [ ] Reviewer skims `legacy/README.md` and `archive/README.md` posture

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-Review

- **Spec coverage:** Codex item 2 ("Quarantine slop") fully covered. Each class label from Codex's list (`product`, `generated`, `legacy`, `reference`, `archive`, `vendored`, `runtime-artifact`) is preserved through the moves. ✓
- **Placeholder scan:** No `TBD` or "implement later". Each task names exact source and destination paths. ✓
- **Type consistency:** All path moves use `legacy/<original_top_dir>/<subdir>` format consistently. ✓
- **No deletion:** Confirmed — every operation is `git mv`. Cargo manifest entries may be removed (those are not files), but file content stays in repo. ✓
- **Build verification per task:** Every task ends with `cargo check --workspace` before commit. ✓

## Risks and mitigations

- **Risk:** A workspace member move silently breaks a downstream consumer outside the workspace.
  - **Mitigation:** `cargo check --workspace` after each move; the audit diff catches LOC drift; `grep` for old paths in docs.
- **Risk:** A `git mv` operation that crosses filesystem boundaries fails.
  - **Mitigation:** All moves are within the same repo root, no cross-fs concerns.
- **Risk:** A consumer relies on Python import path `apps.heiwa_hub.X`.
  - **Mitigation:** Hub is legacy; if a product crate imports it, that import is itself a bug and should be removed before this plan lands.

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-04-25-slop-quarantine.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
