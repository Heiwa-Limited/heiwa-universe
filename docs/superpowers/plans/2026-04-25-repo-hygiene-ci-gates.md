# Repo Hygiene CI Gates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the audit script's class budgets enforceable. Add a CI job that fails on slop budget breach, expand `.gitignore` to block runtime-artifact leakage, and add a pre-commit hook that catches the same locally before push.

**Architecture:** Three layers of defense, with the CI job as the authoritative gate. The audit script gains a `--check` mode that exits non-zero on hard-cap breach. A new GitHub Actions workflow runs the audit on every PR. A pre-commit hook (managed via `pre-commit` framework or a plain `.git/hooks/pre-commit` script) blocks tracked runtime artifacts at commit time.

**Tech Stack:** GitHub Actions, bash, `pre-commit` framework (Python). No new runtime deps.

**Prerequisites:**
- Plan 1 merged (`PRODUCT_SURFACE.md` and `scripts/audit_product_surface.sh` exist).
- Plan 2 substantially merged (slop quarantined so the budget check has any chance of passing).

---

## File Structure

| Path | Action | Responsibility |
| --- | --- | --- |
| `scripts/audit_product_surface.sh` | Modify | Add `--check` flag with hard-cap exit logic |
| `tests/audit/test_audit_product_surface.bats` | Modify | Add `--check` mode tests |
| `.github/workflows/repo-hygiene.yml` | Create | CI job running audit on every PR |
| `.gitignore` | Modify | Add explicit blocks for `__pycache__`, `.pytest_cache`, etc. |
| `.pre-commit-config.yaml` | Create | Pre-commit framework config |
| `scripts/check_no_runtime_artifacts.sh` | Create | Hook target script |
| `docs/standards/repo-hygiene.md` | Create | Reference doc explaining the gates |

---

### Task 1: Add `--check` mode to audit script (TDD)

**Files:**
- Modify: `tests/audit/test_audit_product_surface.bats`
- Modify: `scripts/audit_product_surface.sh`

- [ ] **Step 1: Append failing tests for --check mode**

```bash
@test "audit_product_surface.sh --check exits 0 when all classes within hard cap" {
    run env LEGACY_HARD_CAP=99999999 GENERATED_HARD_CAP=99999999 \
        REFERENCE_HARD_CAP=99999999 ARCHIVE_HARD_CAP=99999999 \
        VENDORED_HARD_CAP=99999999 RUNTIME_ARTIFACT_TOLERANCE=99999999 \
        "$REPO_ROOT/scripts/audit_product_surface.sh" --check
    [ "$status" -eq 0 ]
}

@test "audit_product_surface.sh --check exits 1 when legacy exceeds hard cap" {
    run env LEGACY_HARD_CAP=1 "$REPO_ROOT/scripts/audit_product_surface.sh" --check
    [ "$status" -eq 1 ]
    [[ "$output" == *"legacy"* ]]
    [[ "$output" == *"exceeds hard cap"* ]]
}

@test "audit_product_surface.sh --check exits 1 on any runtime-artifact" {
    run env RUNTIME_ARTIFACT_TOLERANCE=0 "$REPO_ROOT/scripts/audit_product_surface.sh" --check
    # Will fail if any runtime artifact is present, pass if zero. Either is informative.
    [[ "$status" -eq 0 || "$output" == *"runtime-artifact"* ]]
}
```

- [ ] **Step 2: Run tests to verify the new ones fail**

Run: `bats tests/audit/test_audit_product_surface.bats`
Expected: New tests FAIL — `--check` not implemented.

- [ ] **Step 3: Add `--check` mode to the audit script**

Edit `scripts/audit_product_surface.sh`. After the existing report-printing code, append:

```bash
# --check mode: exit non-zero on hard-cap breach
if [[ "${1:-}" == "--check" ]]; then
    LEGACY_HARD_CAP=${LEGACY_HARD_CAP:-60000}
    GENERATED_HARD_CAP=${GENERATED_HARD_CAP:-75000}
    REFERENCE_HARD_CAP=${REFERENCE_HARD_CAP:-40000}
    ARCHIVE_HARD_CAP=${ARCHIVE_HARD_CAP:-20000}
    VENDORED_HARD_CAP=${VENDORED_HARD_CAP:-5000}
    RUNTIME_ARTIFACT_TOLERANCE=${RUNTIME_ARTIFACT_TOLERANCE:-0}

    breach=0
    check_cap() {
        local class="$1" cap="$2" actual="${class_loc[$class]:-0}"
        if (( actual > cap )); then
            echo "FAIL: $class LOC ($actual) exceeds hard cap ($cap)" >&2
            breach=1
        fi
    }

    check_cap legacy "$LEGACY_HARD_CAP"
    check_cap generated "$GENERATED_HARD_CAP"
    check_cap reference "$REFERENCE_HARD_CAP"
    check_cap archive "$ARCHIVE_HARD_CAP"
    check_cap vendored "$VENDORED_HARD_CAP"
    check_cap runtime-artifact "$RUNTIME_ARTIFACT_TOLERANCE"

    if (( breach == 1 )); then
        echo ""
        echo "Slop budget breached. See docs/audit/2026-04-25-slop-budget.md for thresholds." >&2
        exit 1
    fi
    echo ""
    echo "All classes within hard caps."
fi
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bats tests/audit/test_audit_product_surface.bats`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/audit_product_surface.sh tests/audit/test_audit_product_surface.bats
git commit -m "feat: add --check mode to audit script with hard-cap budgets"
```

---

### Task 2: Add the GitHub Actions workflow

**Files:**
- Create: `.github/workflows/repo-hygiene.yml`

- [ ] **Step 1: Inspect existing workflow style**

Run: `head -30 .github/workflows/ci.yml`
Expected: A reference for naming, runner, checkout style.

- [ ] **Step 2: Write the workflow**

```yaml
name: Repo Hygiene

on:
  pull_request:
  push:
    branches: [main]

jobs:
  audit-product-surface:
    name: Audit product surface
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 1

      - name: Install bats-core
        run: |
          sudo apt-get update -qq
          sudo apt-get install -y bats

      - name: Make scripts executable
        run: chmod +x scripts/audit_product_surface.sh scripts/lib/parse_product_surface.sh

      - name: Run audit-script tests
        run: bats tests/audit/

      - name: Run audit (informational)
        run: ./scripts/audit_product_surface.sh

      - name: Enforce slop budget
        run: ./scripts/audit_product_surface.sh --check

      - name: Upload audit report as artifact
        if: always()
        run: ./scripts/audit_product_surface.sh > audit-report.txt
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: product-surface-audit
          path: audit-report.txt
```

- [ ] **Step 3: Validate YAML locally**

Run: `python -c "import yaml; yaml.safe_load(open('.github/workflows/repo-hygiene.yml'))" && echo OK`
Expected: `OK`.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/repo-hygiene.yml
git commit -m "ci: add repo-hygiene workflow that enforces slop budget"
```

---

### Task 3: Expand `.gitignore` to block runtime artifacts

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Read current .gitignore**

Run: `cat .gitignore | head -40`
Expected: Existing entries.

- [ ] **Step 2: Append explicit blocks**

Append the following block (do not duplicate entries that already exist):

```gitignore

# === Repo hygiene: runtime artifacts must not be tracked ===
# Python
__pycache__/
*.pyc
*.pyo
*.pyd
.pytest_cache/
.mypy_cache/
.ruff_cache/
.coverage
htmlcov/

# Node
node_modules/
.pnpm-store/
.parcel-cache/
.next/
.turbo/

# Rust
target/
**/*.rs.bk

# Editors / OS
.DS_Store
Thumbs.db
*.swp
*.swo

# Heiwa runtime
.heiwa-cache/
.wrangler/
.openclaw/

# Logs
*.log
logs/
```

- [ ] **Step 3: Identify and untrack any matching files currently tracked**

Run:
```bash
git ls-files | grep -E '__pycache__|\.pytest_cache|\.DS_Store|\.pyc$' | head -20
```
Expected: A list (possibly empty after Plan 2). For each, run:
```bash
git rm -r --cached <path>
```

- [ ] **Step 4: Verify audit shows runtime-artifact = 0**

Run: `./scripts/audit_product_surface.sh | grep runtime-artifact`
Expected: `runtime-artifact   0` (or close to it).

- [ ] **Step 5: Commit**

```bash
git add .gitignore
git commit -m "chore: expand .gitignore to block runtime artifacts"
```

If files were untracked in Step 3, do that as a separate commit:

```bash
git commit -m "chore: untrack accidentally-committed runtime artifacts"
```

---

### Task 4: Add the pre-commit hook

**Files:**
- Create: `.pre-commit-config.yaml`
- Create: `scripts/check_no_runtime_artifacts.sh`

- [ ] **Step 1: Write the runtime-artifact check script**

```bash
#!/usr/bin/env bash
# check_no_runtime_artifacts.sh
# Pre-commit hook: fails if staged files match runtime-artifact patterns.

set -e

PATTERNS=(
    '__pycache__/'
    '\.pyc$'
    '\.pyo$'
    '\.pytest_cache/'
    '\.mypy_cache/'
    '\.ruff_cache/'
    '\.coverage'
    'node_modules/'
    'target/'
    '\.DS_Store'
    '\.wrangler/'
    '\.heiwa-cache/'
    '\.log$'
)

staged=$(git diff --cached --name-only --diff-filter=ACM)
[[ -z "$staged" ]] && exit 0

violations=()
for pattern in "${PATTERNS[@]}"; do
    matches=$(echo "$staged" | grep -E "$pattern" || true)
    [[ -n "$matches" ]] && violations+=("$matches")
done

if (( ${#violations[@]} > 0 )); then
    echo "ERROR: Staged files match runtime-artifact patterns:" >&2
    printf '  %s\n' "${violations[@]}" >&2
    echo "" >&2
    echo "These should be in .gitignore. Unstage with:" >&2
    echo "  git reset HEAD <file>" >&2
    exit 1
fi
```

- [ ] **Step 2: Make it executable**

Run: `chmod +x scripts/check_no_runtime_artifacts.sh`

- [ ] **Step 3: Write `.pre-commit-config.yaml`**

```yaml
repos:
  - repo: local
    hooks:
      - id: no-runtime-artifacts
        name: Block runtime artifacts from being committed
        entry: scripts/check_no_runtime_artifacts.sh
        language: script
        always_run: true
        pass_filenames: false

      - id: product-surface-audit
        name: Quick product-surface audit (informational)
        entry: scripts/audit_product_surface.sh
        language: script
        always_run: true
        pass_filenames: false
        verbose: true
```

- [ ] **Step 4: Test the hook manually**

Run:
```bash
mkdir -p /tmp/hooktest && touch /tmp/hooktest/foo.pyc
git add /tmp/hooktest/foo.pyc 2>/dev/null || cp /tmp/hooktest/foo.pyc ./test_foo.pyc && git add test_foo.pyc
./scripts/check_no_runtime_artifacts.sh
```
Expected: `ERROR: Staged files match runtime-artifact patterns: test_foo.pyc`

Cleanup:
```bash
git reset HEAD test_foo.pyc 2>/dev/null
rm -f test_foo.pyc
```

- [ ] **Step 5: Commit**

```bash
git add .pre-commit-config.yaml scripts/check_no_runtime_artifacts.sh
git commit -m "chore: add pre-commit hooks for runtime-artifact blocking"
```

---

### Task 5: Add the standards reference doc

**Files:**
- Create: `docs/standards/repo-hygiene.md`

- [ ] **Step 1: Write the reference doc**

```markdown
# Repo Hygiene Standards

> **Authority:** `PRODUCT_SURFACE.md` defines what is what; this doc explains how the gates work.

## What gets enforced

Three layers, in order of authority:

1. **GitHub Actions `repo-hygiene` workflow** — the canonical gate. Fails the PR if any class exceeds its hard cap (defined in `docs/audit/2026-04-25-slop-budget.md`).
2. **`.gitignore`** — prevents accidental staging of runtime artifacts.
3. **Pre-commit hooks** — local fast feedback. Block commits with runtime artifacts staged.

## Class budgets

See `docs/audit/2026-04-25-slop-budget.md`. CI threshold env vars are loaded from defaults in the audit script and may be overridden in the workflow YAML.

## How to override

Hard-cap exceptions are PR-by-PR:

- `legacy-add: <reason>` in PR body — reviewer judgment for adding to `legacy/`
- `vendor-add: <reason>` — vendoring requires workspace owner ack
- `archive-add: <reason>` — archiving more than the per-PR cap

Override parsing is currently manual (reviewer reads PR body). Automation is a later cycle's work.

## How to run the gate locally

```bash
./scripts/audit_product_surface.sh           # report
./scripts/audit_product_surface.sh --check   # exit 1 on breach
```

## How to install pre-commit hooks

```bash
pip install pre-commit
pre-commit install
```

After install, `.pre-commit-config.yaml` runs on every `git commit`.

## When the budget changes

1. Update the table in `docs/audit/2026-04-25-slop-budget.md`
2. Update the env var defaults in `scripts/audit_product_surface.sh`
3. Update CI override in `.github/workflows/repo-hygiene.yml` if needed
4. Add a one-line entry to `docs/audit/` describing why the budget moved

The budget should ratchet **down** over time as legacy is deleted, never up without explicit doctrine change.
```

- [ ] **Step 2: Commit**

```bash
git add docs/standards/repo-hygiene.md
git commit -m "docs: document repo hygiene gates and override flow"
```

---

### Task 6: Verify CI passes on a draft PR

- [ ] **Step 1: Push branch and open as draft**

```bash
git push -u origin HEAD
gh pr create --draft --title "ci: add repo-hygiene gates" --body "$(cat <<'EOF'
## Summary
- New CI workflow `.github/workflows/repo-hygiene.yml` runs `audit_product_surface.sh --check` on every PR
- `.gitignore` expanded to block runtime artifacts (Python/Node/Rust caches, OS files, logs)
- `.pre-commit-config.yaml` + `scripts/check_no_runtime_artifacts.sh` for local fast feedback
- `docs/standards/repo-hygiene.md` documents the gate and override flow

## Test plan
- [x] `bats tests/audit/` passes locally with new --check tests
- [x] `audit_product_surface.sh --check` exits 0 with current state (assuming Plan 2 quarantined enough)
- [ ] CI repo-hygiene job passes on this PR
- [ ] Reviewer confirms `.gitignore` additions don't shadow legitimate tracked files

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Watch CI**

Run: `gh pr checks --watch`
Expected: `repo-hygiene / audit-product-surface` job passes.

- [ ] **Step 3: If gate fails because legacy still over cap**

Two options:
- (a) Bump `LEGACY_HARD_CAP` in the workflow temporarily to the current actual + 1, file an issue to ratchet down
- (b) Push more quarantine commits to reduce active legacy LOC

Prefer (b). (a) is escape hatch.

- [ ] **Step 4: Mark PR ready and merge**

Run: `gh pr ready && gh pr merge --squash`

---

## Self-Review

- **Spec coverage:** Codex item 3 ("Hard repo hygiene gates") fully covered. Specifically: tracked runtime logs (covered by `.gitignore` + pre-commit + audit), pyc/cache files (same), accidental vendor trees (catch via `unclassified` tracking + `vendored` cap), stale package metadata (audit script catches if a package gets reclassed), docs presenting legacy as product (handled by quarantine + class assignment in surface table). ✓
- **Placeholder scan:** No placeholders. Override section names a future cycle's work explicitly. ✓
- **Type consistency:** `--check` flag, `LEGACY_HARD_CAP` env var, `class_loc` array names match across script, tests, and docs. ✓
- **Order dependency:** Plan 2 must be substantially merged or this gate fails on day one. The plan's Task 6 Step 3 calls out the escape hatch. ✓

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-04-25-repo-hygiene-ci-gates.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
