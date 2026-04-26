# Product Surface Definition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lock the public product boundary by adding `PRODUCT_SURFACE.md` and a working `scripts/audit_product_surface.sh` that classifies every tracked path. This plan is a hard prerequisite for the slop-quarantine plan and the CI-gates plan.

**Architecture:** A single source-of-truth markdown table maps top-level directories → classes (`product`, `generated`, `legacy`, `reference`, `archive`, `vendored`, `runtime-artifact`). A bash script reads that table and walks `git ls-files` output, emitting LOC totals per class. The script's exit code is informational on this plan; CI enforcement is added in the gates plan.

**Tech Stack:** Bash 5+, `git`, `awk`, `wc`. Bats-core for shell tests (already declared as a dev dep candidate; install via Homebrew if missing). No new runtime dependencies for the product itself.

---

## File Structure

| Path | Action | Responsibility |
| --- | --- | --- |
| `PRODUCT_SURFACE.md` | Create (repo root) | Canonical class table; referenced from `HEIWA.md` |
| `scripts/audit_product_surface.sh` | Create | Reads class table, walks tracked files, emits per-class LOC |
| `scripts/lib/parse_product_surface.sh` | Create | Pure parser fn; sourced by audit + tests |
| `tests/audit/test_audit_product_surface.bats` | Create | Bats tests for parser + script behavior |
| `tests/audit/fixtures/sample_surface.md` | Create | Minimal fixture for parser tests |
| `HEIWA.md` | Modify (`apps/heiwa_shell` table area) | Add 1-line link to `PRODUCT_SURFACE.md` |

Five new files plus one small `HEIWA.md` edit. Self-contained, no Cargo/Python dep changes.

---

### Task 1: Create the fixture for parser tests

**Files:**
- Create: `tests/audit/fixtures/sample_surface.md`

- [ ] **Step 1: Write the fixture file**

```markdown
# Sample Product Surface

| Path | Class |
| --- | --- |
| `crates/example` | product |
| `apps/example_legacy` | legacy |
| `packages/example_gen` | generated |
| `docs/example_ref` | reference |
```

- [ ] **Step 2: Verify the file exists**

Run: `cat tests/audit/fixtures/sample_surface.md | wc -l`
Expected: `7` (or 8 if trailing newline)

- [ ] **Step 3: Commit**

```bash
git add tests/audit/fixtures/sample_surface.md
git commit -m "test: add product-surface parser fixture"
```

---

### Task 2: Write the failing parser test

**Files:**
- Create: `tests/audit/test_audit_product_surface.bats`

- [ ] **Step 1: Install bats-core if missing**

Run: `command -v bats || brew install bats-core`
Expected: bats command resolves; if installed via brew, version printed.

- [ ] **Step 2: Write the failing parser test**

```bash
#!/usr/bin/env bats

setup() {
    REPO_ROOT="$(git rev-parse --show-toplevel)"
    source "$REPO_ROOT/scripts/lib/parse_product_surface.sh"
}

@test "parse_product_surface extracts path -> class pairs from sample fixture" {
    local fixture="$REPO_ROOT/tests/audit/fixtures/sample_surface.md"
    run parse_product_surface "$fixture"
    [ "$status" -eq 0 ]
    [[ "$output" == *"crates/example product"* ]]
    [[ "$output" == *"apps/example_legacy legacy"* ]]
    [[ "$output" == *"packages/example_gen generated"* ]]
    [[ "$output" == *"docs/example_ref reference"* ]]
}

@test "parse_product_surface emits one path per line" {
    local fixture="$REPO_ROOT/tests/audit/fixtures/sample_surface.md"
    run parse_product_surface "$fixture"
    [ "$status" -eq 0 ]
    [ "$(echo "$output" | wc -l | tr -d ' ')" = "4" ]
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `bats tests/audit/test_audit_product_surface.bats`
Expected: FAIL with "parse_product_surface: command not found" or "no such file" for `scripts/lib/parse_product_surface.sh`.

- [ ] **Step 4: Commit**

```bash
git add tests/audit/test_audit_product_surface.bats
git commit -m "test: add failing parser tests for product surface"
```

---

### Task 3: Implement the parser to pass tests

**Files:**
- Create: `scripts/lib/parse_product_surface.sh`

- [ ] **Step 1: Implement minimal parser**

```bash
#!/usr/bin/env bash
# parse_product_surface SURFACE_FILE
# Emits "<path> <class>" lines, one per row in the surface markdown table.

parse_product_surface() {
    local surface_file="$1"
    if [[ ! -f "$surface_file" ]]; then
        echo "parse_product_surface: file not found: $surface_file" >&2
        return 1
    fi

    awk '
        /^\| `/ {
            gsub(/`/, "")
            split($0, fields, "|")
            path = fields[2]; sub(/^ +| +$/, "", path); gsub(/ /, "", path)
            class = fields[3]; sub(/^ +| +$/, "", class); gsub(/ /, "", class)
            if (path != "" && class != "" && class != "Class") {
                print path, class
            }
        }
    ' "$surface_file"
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `bats tests/audit/test_audit_product_surface.bats`
Expected: 2 tests, both PASS.

- [ ] **Step 3: Commit**

```bash
git add scripts/lib/parse_product_surface.sh
git commit -m "feat: implement product-surface markdown parser"
```

---

### Task 4: Write the failing audit-script test

**Files:**
- Modify: `tests/audit/test_audit_product_surface.bats:end`

- [ ] **Step 1: Append the audit-script test**

```bash
@test "audit_product_surface.sh reports per-class LOC totals" {
    run "$REPO_ROOT/scripts/audit_product_surface.sh"
    [ "$status" -eq 0 ]
    [[ "$output" == *"product"* ]]
    [[ "$output" == *"legacy"* ]]
    [[ "$output" == *"LOC"* ]]
}

@test "audit_product_surface.sh exits non-zero when surface file is missing" {
    run env SURFACE_FILE=/nonexistent.md "$REPO_ROOT/scripts/audit_product_surface.sh"
    [ "$status" -ne 0 ]
}
```

- [ ] **Step 2: Run tests to verify the new ones fail**

Run: `bats tests/audit/test_audit_product_surface.bats`
Expected: 2 prior PASS, 2 new FAIL ("scripts/audit_product_surface.sh: not found").

- [ ] **Step 3: Commit**

```bash
git add tests/audit/test_audit_product_surface.bats
git commit -m "test: add failing audit-script integration tests"
```

---

### Task 5: Create the canonical PRODUCT_SURFACE.md

**Files:**
- Create: `PRODUCT_SURFACE.md` (repo root)

- [ ] **Step 1: Write the surface document**

```markdown
# Product Surface

> **Canonical map of repo paths to classes.** Read by `scripts/audit_product_surface.sh`. Update this file when classes shift; do not change classes without updating `docs/audit/2026-04-25-slop-budget.md`.

**Last updated:** 2026-04-25
**Authority:** `HEIWA.md` defines what is product. This file labels every tracked top-level path.

## Classes

| Class | Meaning |
| --- | --- |
| `product` | Active surfaces shipping in `heiwa` binary, companion app, or sub-products |
| `generated` | Code emitted from a registered generator (bindings, schema-derived) |
| `legacy` | Old surfaces kept for migration/reference; not in product contract |
| `reference` | Plans, design docs, audits, ADRs |
| `archive` | Frozen snapshots of removed work |
| `vendored` | Third-party code copied in (rare) |
| `runtime-artifact` | Logs, caches, tmp data — must not be tracked |

## Path → class table

| Path | Class |
| --- | --- |
| `crates` | product |
| `apps/heiwa_shell` | product |
| `apps/heiwa_core` | product |
| `apps/heiwa_app` | product |
| `apps/heiwa_orchestrator` | product |
| `apps/heiwa_trading` | product |
| `apps/heiwa_hub` | legacy |
| `apps/heiwa_cli` | legacy |
| `apps/heiwa_limbs` | legacy |
| `apps/heiwa_dj` | archive |
| `packages/heiwa_sdk` | product |
| `packages/heiwa_protocol` | product |
| `packages/heiwa_cli` | product |
| `packages/heiwa_identity` | product |
| `packages/heiwa_bindings` | generated |
| `packages/heiwa_skills` | legacy |
| `packages/heiwa_cognition` | legacy |
| `packages/heiwa_ui` | legacy |
| `docs/superpowers` | reference |
| `docs/design` | reference |
| `docs/audit` | reference |
| `docs/enterprise` | product |
| `docs/standards` | product |
| `docs` | product |
| `ops` | product |
| `scripts` | product |
| `infra` | product |
| `config` | product |
| `runtime` | runtime-artifact |
| `bin` | product |
| `node` | legacy |
| `policies` | product |
| `tests` | product |
| `memory` | reference |
| `plans` | reference |
| `site` | generated |

## Notes on specific paths

- `apps/heiwa_trading` is a sub-product (Polymarket paper-trading), not slop. Per `CLAUDE.md`, kept active.
- `packages/heiwa_skills` (86k LOC) is the largest single legacy surface. Quarantine plan moves it under `legacy/`.
- `apps/heiwa_hub` (25k LOC) is the legacy Python ops surface. Quarantine plan moves it under `legacy/`.
- `docs/audit` is reference but contains operational baselines — do not delete entries.
- `runtime/` should ideally be split: config under `config/`, logs to `.gitignore`. Tracked as `runtime-artifact` until split.

## How the budget is computed

The audit script walks each tracked file, locates the longest-prefix match in this table, sums LOC per class, and prints a report. Files matching no prefix are reported as `unclassified` and count against a separate budget (target: 0).
```

- [ ] **Step 2: Verify the file parses cleanly**

Run: `bash scripts/lib/parse_product_surface.sh && source scripts/lib/parse_product_surface.sh && parse_product_surface PRODUCT_SURFACE.md | head -10`
Expected: Lines like `crates product`, `apps/heiwa_shell product`, etc.

- [ ] **Step 3: Commit**

```bash
git add PRODUCT_SURFACE.md
git commit -m "docs: add canonical PRODUCT_SURFACE.md class table"
```

---

### Task 6: Implement the audit script to pass tests

**Files:**
- Create: `scripts/audit_product_surface.sh`

- [ ] **Step 1: Write the audit script**

```bash
#!/usr/bin/env bash
# audit_product_surface.sh
# Reads PRODUCT_SURFACE.md, walks tracked files, emits per-class LOC totals.
# Exit 0 normally; exit non-zero when surface file is missing or parse fails.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
SURFACE_FILE="${SURFACE_FILE:-$REPO_ROOT/PRODUCT_SURFACE.md}"

if [[ ! -f "$SURFACE_FILE" ]]; then
    echo "audit_product_surface: surface file not found: $SURFACE_FILE" >&2
    exit 2
fi

# shellcheck source=lib/parse_product_surface.sh
source "$REPO_ROOT/scripts/lib/parse_product_surface.sh"

# Build path:class map (longest-prefix wins on conflict — sort by length desc)
mapping=$(parse_product_surface "$SURFACE_FILE" | awk '{print length($1), $0}' | sort -rn | cut -d' ' -f2-)

declare -A class_loc
declare -i unclassified_loc=0

while IFS= read -r file; do
    # Skip empty lines
    [[ -z "$file" ]] && continue
    [[ ! -f "$REPO_ROOT/$file" ]] && continue

    # Count LOC for this file
    loc=$(wc -l < "$REPO_ROOT/$file" 2>/dev/null || echo 0)

    # Find longest-prefix class
    matched_class=""
    while IFS=' ' read -r path class; do
        if [[ "$file" == "$path"* ]]; then
            matched_class="$class"
            break
        fi
    done <<< "$mapping"

    if [[ -n "$matched_class" ]]; then
        class_loc[$matched_class]=$((${class_loc[$matched_class]:-0} + loc))
    else
        unclassified_loc=$((unclassified_loc + loc))
    fi
done < <(cd "$REPO_ROOT" && git ls-files)

echo "=== Product Surface Audit ==="
echo "Surface file: $SURFACE_FILE"
echo ""
printf "%-20s %12s\n" "CLASS" "LOC"
printf "%-20s %12s\n" "-----" "---"
for class in product generated legacy reference archive vendored runtime-artifact; do
    printf "%-20s %12s\n" "$class" "${class_loc[$class]:-0}"
done
printf "%-20s %12s\n" "unclassified" "$unclassified_loc"
```

- [ ] **Step 2: Make it executable**

Run: `chmod +x scripts/audit_product_surface.sh && chmod +x scripts/lib/parse_product_surface.sh`
Expected: No output; both files now executable.

- [ ] **Step 3: Run tests to verify all pass**

Run: `bats tests/audit/test_audit_product_surface.bats`
Expected: 4 tests, all PASS.

- [ ] **Step 4: Run the script manually to spot-check**

Run: `./scripts/audit_product_surface.sh`
Expected output starts with `=== Product Surface Audit ===` and shows non-zero LOC for `product`, `legacy`, `generated`, `reference`. `unclassified` should be small (< 1000).

- [ ] **Step 5: Commit**

```bash
git add scripts/audit_product_surface.sh scripts/lib/parse_product_surface.sh
git commit -m "feat: add audit_product_surface.sh"
```

---

### Task 7: Triage `unclassified` LOC down to zero

**Files:**
- Modify: `PRODUCT_SURFACE.md` (add missing prefixes as needed)

- [ ] **Step 1: Identify unclassified files**

Run a small helper inline:

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
source "$REPO_ROOT/scripts/lib/parse_product_surface.sh"
mapping=$(parse_product_surface "$REPO_ROOT/PRODUCT_SURFACE.md" | awk '{print length($1), $0}' | sort -rn | cut -d' ' -f2-)

cd "$REPO_ROOT" && git ls-files | while read -r file; do
    matched=""
    while IFS=' ' read -r path class; do
        if [[ "$file" == "$path"* ]]; then matched=1; break; fi
    done <<< "$mapping"
    [[ -z "$matched" ]] && echo "$file"
done | head -50
```

Expected: A list of files not covered by any prefix. Common candidates: top-level files like `Cargo.toml`, `package.json`, `README.md`, `LICENSE`, etc.

- [ ] **Step 2: Add a catch-all for repo-root files**

Edit `PRODUCT_SURFACE.md` and add (in the table) any missing top-level `*.md`, `*.toml`, `*.json`, `*.lock` entries with class `product` (these are workspace metadata) or `reference` (READMEs). Add specific paths only — no wildcard fallback.

Example additions:
```markdown
| `Cargo.toml` | product |
| `Cargo.lock` | generated |
| `package.json` | product |
| `package-lock.json` | generated |
| `pyproject.toml` | product |
| `uv.lock` | generated |
| `README.md` | product |
| `LICENSE` | product |
| `HEIWA.md` | product |
| `CLAUDE.md` | product |
| `AGENTS.md` | product |
| `GEMINI.md` | product |
| `IDENTITY.md` | reference |
| `SOUL.md` | reference |
| `SECURITY.md` | product |
| `CONTRIBUTING.md` | product |
| `CONTRIBUTORS.md` | product |
| `CODE_OF_CONDUCT.md` | product |
| `BUILD_MATRIX.md` | reference |
| `mkdocs.yml` | product |
| `biome.json` | product |
| `tsconfig.base.json` | product |
| `rust-toolchain.toml` | product |
| `requirements.txt` | product |
| `conftest.py` | product |
| `justfile` | product |
| `PRODUCT_SURFACE.md` | product |
```

- [ ] **Step 3: Re-run audit and verify unclassified is zero or near-zero**

Run: `./scripts/audit_product_surface.sh`
Expected: `unclassified` row shows `0` or a very small number. If non-zero, repeat Step 2.

- [ ] **Step 4: Commit**

```bash
git add PRODUCT_SURFACE.md
git commit -m "docs: classify all tracked top-level files in PRODUCT_SURFACE.md"
```

---

### Task 8: Link from HEIWA.md

**Files:**
- Modify: `HEIWA.md` (around the "Canonical Product Identity" section, ~line 120)

- [ ] **Step 1: Read current section**

Run: `grep -n "Canonical Product Identity" HEIWA.md`
Expected: Line ~120.

- [ ] **Step 2: Insert link line after the table**

Edit `HEIWA.md` to add immediately after the canonical-identity table closing line:

```markdown

> See [`PRODUCT_SURFACE.md`](PRODUCT_SURFACE.md) for the path-by-path class table that is the input to repo-hygiene CI.
```

- [ ] **Step 3: Verify the link resolves**

Run: `grep -A 1 "PRODUCT_SURFACE.md" HEIWA.md`
Expected: The new line appears.

- [ ] **Step 4: Commit**

```bash
git add HEIWA.md
git commit -m "docs: link PRODUCT_SURFACE.md from HEIWA.md canonical identity"
```

---

### Task 9: Final verification

- [ ] **Step 1: Run all bats tests**

Run: `bats tests/audit/`
Expected: 4 tests, 4 PASS.

- [ ] **Step 2: Run the audit and capture output**

Run: `./scripts/audit_product_surface.sh > /tmp/surface_audit.txt && cat /tmp/surface_audit.txt`
Expected: Per-class LOC report. `product` should be roughly 40k-60k. `legacy` should be ~120k pre-quarantine. `unclassified` should be 0.

- [ ] **Step 3: Confirm no untracked files left**

Run: `git status`
Expected: Clean working tree.

- [ ] **Step 4: Push branch and open PR**

```bash
git push -u origin HEAD
gh pr create --title "feat: define product surface boundary + audit script" --body "$(cat <<'EOF'
## Summary
- Adds `PRODUCT_SURFACE.md` mapping every tracked top-level path to a class
- Adds `scripts/audit_product_surface.sh` to compute per-class LOC totals
- Adds bats tests covering the parser and the audit script
- Links the new doc from `HEIWA.md`

## Test plan
- [x] `bats tests/audit/` passes
- [x] `./scripts/audit_product_surface.sh` runs cleanly and shows zero unclassified
- [ ] Reviewer confirms class assignments match `HEIWA.md` doctrine

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-Review

- **Spec coverage:** Codex item 1 ("Define the public product boundary") fully covered. The classes match Codex's stated taxonomy. ✓
- **Placeholder scan:** No `TBD`, `implement later`, or `add appropriate error handling`. All steps have concrete commands and code. ✓
- **Type consistency:** `parse_product_surface` is the function name in lib, test, and audit script. Class names (`product`, `legacy`, etc.) are consistent across PRODUCT_SURFACE.md, audit script, and budget report. ✓
- **No deletion:** This plan only adds files and edits HEIWA.md by appending. No existing surfaces touched. ✓

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-04-25-product-surface-definition.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
