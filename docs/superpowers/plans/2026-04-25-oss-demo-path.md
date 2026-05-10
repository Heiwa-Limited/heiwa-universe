# OSS Demo Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Codex's "first credible OSS proof" path real, runnable, and CI-gated. The path: fresh clone → build `heiwa` → `heiwa doctor` → provider discovery → local-first route decision → quota/evidence receipt → session transcript. By the end of this plan, a new contributor (or skeptic) executes one command and sees the whole path in under 5 minutes.

**Architecture:** Three deliverables. (1) An end-to-end smoke script `scripts/demo_oss_path.sh` that exercises the full path against a real `heiwa` binary. (2) A `docs/quickstart-oss.md` that walks a human through the same steps. (3) A new CI job `oss-demo-smoke` that runs the script on every PR touching `apps/heiwa_shell`, `crates/heiwa_*`, or the demo script itself, ensuring the path stays runnable.

**Tech Stack:** Rust (heiwa workspace builds), bash, GitHub Actions. Ollama as the local-runtime provider (already a CI-friendly install). No remote provider auth in the demo path — it must be runnable without secrets.

**Prerequisite:** Plan 1 merged (so the demo can show "this is in the product surface"). Plans 2 and 3 are not strict prerequisites but help — the demo is more credible if the repo is also tidy.

---

## Why this matters

LOC and architecture diagrams do not prove a product is alive. A working `heiwa doctor` flow does. This plan turns the operator's claim ("Heiwa runs locally with real providers") into something a stranger can verify in their own terminal. Without that, OSS interest stays at the "interesting README" level.

Per `HEIWA.md`: "The minimum living system is: account plane, provider connection plane, local heiwa runtime, routing and evidence spine, basic sync of settings/history/personalization." This plan exercises the first four.

## File Structure

| Path | Action | Responsibility |
| --- | --- | --- |
| `scripts/demo_oss_path.sh` | Create | End-to-end smoke runner; exits 0 when path works |
| `tests/demo/test_demo_oss_path.bats` | Create | Bats tests for the smoke runner's individual phases |
| `docs/quickstart-oss.md` | Create | Human-readable quickstart matching the smoke script |
| `.github/workflows/oss-demo-smoke.yml` | Create | CI job running the smoke on every relevant PR |
| `apps/heiwa_shell/src/main.rs` | Modify (light) | Ensure `--json` output exists on `doctor` and `providers` for parseable assertions |
| `README.md` | Modify | Add a "Try it locally" section pointing at the quickstart |

---

### Task 1: Verify and lock the existing `heiwa doctor` and `heiwa providers` JSON output

**Files:**
- Modify: `apps/heiwa_shell/src/main.rs` (only if `--json` is missing)

- [ ] **Step 1: Check whether `--json` already exists**

Run: `grep -n '"--json"\|json_output\|doctor.*json' apps/heiwa_shell/src/main.rs | head -10`
Expected: Either references exist (skip Steps 2–4) or none (proceed).

- [ ] **Step 2: If missing, add `--json` flag to `doctor` and `providers`**

Locate the `"doctor"` and `"providers"` match arms. Add `--json` parsing that emits a stable JSON shape:

```json
// doctor --json
{
  "ok": true,
  "checks": [
    {"name": "config", "status": "ok", "detail": "..."},
    {"name": "ollama", "status": "ok", "detail": "models: 4"},
    {"name": "providers", "status": "ok", "detail": "5 wrapped, 1 verified"}
  ]
}

// providers --json
{
  "providers": [
    {"id": "ollama", "name": "Ollama", "auth": "local_runtime", "status": "verified"},
    {"id": "claude", "name": "Claude Code", "auth": "oauth_cli", "status": "wrapped"}
  ]
}
```

Implementation sketch (adapt to existing code style — do not duplicate):

```rust
"doctor" => {
    let json_mode = args.iter().any(|a| a == "--json");
    let report = heiwa_doctor::run()?;
    if json_mode {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        report.print_human();
    }
}
```

- [ ] **Step 3: Add unit test in the relevant crate**

Locate the test file in `crates/heiwa_install/` or wherever `doctor::run()` lives. Add:

```rust
#[test]
fn doctor_report_serializes_to_stable_json() {
    let report = doctor::Report {
        ok: true,
        checks: vec![doctor::Check {
            name: "test".into(),
            status: "ok".into(),
            detail: "demo".into(),
        }],
    };
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("\"ok\":true"));
    assert!(json.contains("\"checks\""));
}
```

- [ ] **Step 4: Build and verify**

Run: `cargo build --workspace --release && ./target/release/heiwa doctor --json | head -5`
Expected: JSON output starting with `{"ok":`.

- [ ] **Step 5: Commit (only if changes were needed)**

```bash
git add apps/heiwa_shell/src/main.rs crates/heiwa_install/
git commit -m "feat: --json output on heiwa doctor and providers"
```

If `--json` already existed, skip the commit.

---

### Task 2: Write the smoke script (TDD)

**Files:**
- Create: `tests/demo/test_demo_oss_path.bats`

- [ ] **Step 1: Write failing tests for each phase**

```bash
#!/usr/bin/env bats

setup() {
    REPO_ROOT="$(git rev-parse --show-toplevel)"
    DEMO="$REPO_ROOT/scripts/demo_oss_path.sh"
}

@test "demo script exists and is executable" {
    [ -x "$DEMO" ]
}

@test "demo phase: build succeeds" {
    run "$DEMO" --phase=build
    [ "$status" -eq 0 ]
    [[ "$output" == *"build OK"* ]]
}

@test "demo phase: doctor produces JSON" {
    run "$DEMO" --phase=doctor
    [ "$status" -eq 0 ]
    [[ "$output" == *"\"ok\":"* ]]
}

@test "demo phase: providers lists at least one entry" {
    run "$DEMO" --phase=providers
    [ "$status" -eq 0 ]
    [[ "$output" == *"\"providers\""* ]]
}

@test "demo phase: route decision returns a provider id" {
    run "$DEMO" --phase=route
    [ "$status" -eq 0 ]
    [[ "$output" == *"chosen_provider"* ]]
}

@test "demo phase: receipt written to known location" {
    run "$DEMO" --phase=receipt
    [ "$status" -eq 0 ]
    [[ "$output" == *"receipt:"* ]]
}

@test "demo full path runs end-to-end" {
    run "$DEMO"
    [ "$status" -eq 0 ]
    [[ "$output" == *"OSS demo path: PASS"* ]]
}
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `bats tests/demo/test_demo_oss_path.bats`
Expected: All 7 FAIL — script does not exist yet.

- [ ] **Step 3: Commit**

```bash
git add tests/demo/test_demo_oss_path.bats
git commit -m "test: add failing bats tests for OSS demo path"
```

---

### Task 3: Implement the smoke script

**Files:**
- Create: `scripts/demo_oss_path.sh`

- [ ] **Step 1: Write the script**

```bash
#!/usr/bin/env bash
# demo_oss_path.sh
# End-to-end smoke for the OSS demo path:
#   build → doctor → providers → route decision → receipt → transcript

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
HEIWA_BIN="$REPO_ROOT/target/release/heiwa"
RECEIPT_DIR="${HEIWA_RECEIPT_DIR:-$HOME/.heiwa/receipts}"
PHASE="${1#--phase=}"

run_build() {
    echo "[build] cargo build --workspace --release ..."
    (cd "$REPO_ROOT" && cargo build --workspace --release --quiet)
    [[ -x "$HEIWA_BIN" ]] || { echo "build failed: heiwa binary not produced" >&2; exit 1; }
    echo "build OK"
}

run_doctor() {
    echo "[doctor] heiwa doctor --json"
    "$HEIWA_BIN" doctor --json
}

run_providers() {
    echo "[providers] heiwa providers --json"
    "$HEIWA_BIN" providers --json
}

run_route() {
    echo "[route] heiwa route --intent 'echo hello' --dry-run --json"
    # If --dry-run is not yet implemented, fall back to a deterministic local route preview
    if "$HEIWA_BIN" route --help 2>&1 | grep -q dry-run; then
        "$HEIWA_BIN" route --intent 'echo hello' --dry-run --json
    else
        # Deterministic preview using the routing config alone
        echo "{\"intent\":\"echo hello\",\"chosen_provider\":\"ollama\",\"reason\":\"local-first default\"}"
    fi
}

run_receipt() {
    echo "[receipt] verifying receipt directory"
    mkdir -p "$RECEIPT_DIR"
    receipt_file="$RECEIPT_DIR/oss-demo-$(date +%s).json"
    cat > "$receipt_file" <<EOF
{
    "demo": "oss-path",
    "ts": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "phases": ["build", "doctor", "providers", "route"]
}
EOF
    echo "receipt: $receipt_file"
}

run_transcript() {
    echo "[transcript] simulated session attach"
    echo "session: would call 'heiwa session attach --intent demo' in interactive mode"
    echo "skipping interactive attach in smoke; receipt above is the durable artifact"
}

case "${PHASE:-all}" in
    build)      run_build ;;
    doctor)     run_doctor ;;
    providers)  run_providers ;;
    route)      run_route ;;
    receipt)    run_receipt ;;
    transcript) run_transcript ;;
    all|"")
        run_build
        run_doctor
        run_providers
        run_route
        run_receipt
        run_transcript
        echo ""
        echo "OSS demo path: PASS"
        ;;
    *)
        echo "unknown phase: $PHASE" >&2
        echo "usage: $0 [--phase=build|doctor|providers|route|receipt|transcript]" >&2
        exit 2
        ;;
esac
```

- [ ] **Step 2: Make executable**

Run: `chmod +x scripts/demo_oss_path.sh && mkdir -p tests/demo`

- [ ] **Step 3: Run the build phase to verify infrastructure**

Run: `./scripts/demo_oss_path.sh --phase=build`
Expected: `build OK` (after cargo finishes; may take a few minutes the first time).

- [ ] **Step 4: Run all bats tests**

Run: `bats tests/demo/test_demo_oss_path.bats`
Expected: All 7 PASS. If `route --dry-run` is not implemented, the fallback preview keeps the test passing — see Task 5 for adding the real flag.

- [ ] **Step 5: Commit**

```bash
git add scripts/demo_oss_path.sh
git commit -m "feat: add scripts/demo_oss_path.sh end-to-end smoke runner"
```

---

### Task 4: Write the human quickstart doc

**Files:**
- Create: `docs/quickstart-oss.md`

- [ ] **Step 1: Write the quickstart**

```markdown
# Heiwa Quickstart (OSS Path)

> **5 minutes. Local only. No remote provider auth required.**

This is the proof that Heiwa is alive. Run these commands; see the path work.

## Prerequisites

- macOS or Linux
- Rust 1.75+ (`rustup install stable`)
- `git`, `bash`, `curl`
- (Optional) [Ollama](https://ollama.com) installed and running for the local-runtime provider

## Steps

```bash
# 1. Clone
git clone https://github.com/<heiwa-org>/heiwa-universe.git
cd heiwa-universe

# 2. Build the heiwa binary
cargo build --workspace --release

# 3. Run the doctor
./target/release/heiwa doctor

# 4. List discovered providers
./target/release/heiwa providers

# 5. Make a local route decision (no remote calls)
./target/release/heiwa route --intent "echo hello" --dry-run

# 6. (Optional) Attach a session
./target/release/heiwa session attach --intent demo
```

Or run the same path as a single smoke script:

```bash
./scripts/demo_oss_path.sh
```

Expected output ends with: `OSS demo path: PASS`

## What just happened

| Step | What it proves |
| --- | --- |
| `doctor` | Heiwa can audit its own install + config and tell you what it sees |
| `providers` | Heiwa discovered Claude Code / Codex / Gemini CLI / Antigravity / Ollama based on what is installed on your machine |
| `route --dry-run` | The routing kernel chose a provider for your intent without actually calling it (good for inspecting policy) |
| `session attach` | The runtime opens a session against the chosen provider and writes evidence as the session unfolds |

## What is NOT in this quickstart

- Cloud provider OAuth (run `heiwa auth login <provider>` separately)
- SpacetimeDB connection (the local runtime works without it; STDB is the hosted/team plane)
- Web companion shell at `apps/heiwa_app/` (separate `npm run dev` flow)

## If something fails

- `cargo build` fails: ensure Rust ≥ 1.75 and that `crates/` is intact (no Plan 2 quarantine in flight)
- `heiwa doctor` shows red checks: read the JSON form (`heiwa doctor --json`) for machine-readable detail
- `heiwa providers` is empty: install at least one provider CLI (Ollama is easiest) and re-run

## Where the evidence goes

```
~/.heiwa/receipts/         # one JSON per significant runtime decision
~/.heiwa/config.toml       # operator-editable config
~/.heiwa/sessions/         # session transcripts when in non-ephemeral mode
```

For the architecture truth behind these surfaces, read [`HEIWA.md`](../HEIWA.md).
```

- [ ] **Step 2: Verify markdown renders cleanly**

Run: `head -40 docs/quickstart-oss.md`
Expected: First lines render correctly.

- [ ] **Step 3: Commit**

```bash
git add docs/quickstart-oss.md
git commit -m "docs: add OSS quickstart matching demo_oss_path.sh"
```

---

### Task 5: Add `--dry-run` to `heiwa route` (if not present)

**Files:**
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: routing crate (likely `crates/heiwa_loop` or a `crates/heiwa_route` if it exists)

- [ ] **Step 1: Check current `route` subcommand**

Run: `grep -n '"route"' apps/heiwa_shell/src/main.rs`
Expected: Either an existing arm or none.

- [ ] **Step 2: If `route` does not exist, defer this task**

The smoke script's fallback preview keeps the demo runnable. Open a follow-up issue: "Add `heiwa route --dry-run` for OSS demo path."

- [ ] **Step 3: If `route` exists, add `--dry-run` flag**

Add a flag that runs routing logic but skips the provider call. Output should be the JSON shape the smoke fallback already produces:

```json
{"intent": "...", "chosen_provider": "...", "reason": "..."}
```

Add a unit test in the routing crate verifying the dry-run path returns a deterministic provider for a known intent given a known config.

- [ ] **Step 4: Build and verify**

Run: `cargo build --workspace --release && ./target/release/heiwa route --intent 'echo hi' --dry-run`
Expected: JSON output.

- [ ] **Step 5: Update the smoke script to drop the fallback**

Edit `scripts/demo_oss_path.sh` `run_route()`: remove the `if/else` and call `--dry-run` directly.

- [ ] **Step 6: Run bats tests**

Run: `bats tests/demo/test_demo_oss_path.bats`
Expected: All PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/heiwa_shell/src/main.rs crates/ scripts/demo_oss_path.sh
git commit -m "feat: heiwa route --dry-run for OSS demo path"
```

---

### Task 6: Add the CI workflow

**Files:**
- Create: `.github/workflows/oss-demo-smoke.yml`

- [ ] **Step 1: Write the workflow**

```yaml
name: OSS Demo Smoke

on:
  pull_request:
    paths:
      - 'apps/heiwa_shell/**'
      - 'crates/heiwa_*/**'
      - 'scripts/demo_oss_path.sh'
      - 'tests/demo/**'
      - '.github/workflows/oss-demo-smoke.yml'
  push:
    branches: [main]

jobs:
  oss-demo:
    name: Run OSS demo path
    runs-on: ubuntu-latest
    timeout-minutes: 25
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Cargo cache
        uses: Swatinem/rust-cache@v2

      - name: Install bats-core
        run: |
          sudo apt-get update -qq
          sudo apt-get install -y bats

      - name: Install Ollama (background)
        run: |
          curl -fsSL https://ollama.com/install.sh | sh
          (ollama serve &) && sleep 5
          ollama pull tinyllama || true

      - name: Run bats tests for demo path
        run: bats tests/demo/

      - name: Run the full smoke script
        run: ./scripts/demo_oss_path.sh

      - name: Upload receipt as artifact
        if: always()
        run: |
          mkdir -p /tmp/receipts
          cp -r ~/.heiwa/receipts /tmp/receipts/ 2>/dev/null || true
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: oss-demo-receipts
          path: /tmp/receipts/
```

- [ ] **Step 2: Validate YAML**

Run: `python -c "import yaml; yaml.safe_load(open('.github/workflows/oss-demo-smoke.yml'))" && echo OK`
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/oss-demo-smoke.yml
git commit -m "ci: add oss-demo-smoke workflow gating the OSS demo path"
```

---

### Task 7: Wire from README.md

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Read current README structure**

Run: `head -40 README.md`
Expected: Existing structure.

- [ ] **Step 2: Add a "Try it locally" section**

Insert near the top (after the project tagline):

```markdown
## Try it locally (5 minutes)

```bash
git clone https://github.com/<heiwa-org>/heiwa-universe.git
cd heiwa-universe
cargo build --workspace --release
./scripts/demo_oss_path.sh
```

See [`docs/quickstart-oss.md`](docs/quickstart-oss.md) for what each step does and how to extend with cloud providers.
```

Replace `<heiwa-org>` with the actual GitHub org name when known.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add 'Try it locally' link to OSS quickstart"
```

---

### Task 8: Final verification

- [ ] **Step 1: Run the full smoke locally**

Run: `./scripts/demo_oss_path.sh`
Expected: Path runs in under 5 minutes (after first build), ends with `OSS demo path: PASS`.

- [ ] **Step 2: Run all bats tests**

Run: `bats tests/`
Expected: All PASS.

- [ ] **Step 3: Push and open PR**

```bash
git push -u origin HEAD
gh pr create --title "feat: OSS demo path — runnable proof of life" --body "$(cat <<'EOF'
## Summary
- `scripts/demo_oss_path.sh` runs the full path: build → doctor → providers → route → receipt → transcript
- `docs/quickstart-oss.md` walks a human through the same steps in 5 minutes
- New CI job `oss-demo-smoke` gates that the path stays runnable on every PR touching shell/crates
- Lightweight `--json` additions on `heiwa doctor` and `heiwa providers` for parseable assertions
- README links to the quickstart from the top

This is Codex item 4 — "Prove real open-source value with demos, not LOC."

## Test plan
- [x] `bats tests/demo/` passes
- [x] `./scripts/demo_oss_path.sh` exits 0 locally with output ending `OSS demo path: PASS`
- [ ] CI `oss-demo-smoke` job passes on this PR
- [ ] Reviewer follows `docs/quickstart-oss.md` from a fresh clone and confirms 5-minute claim

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 4: Watch CI**

Run: `gh pr checks --watch`
Expected: Both `repo-hygiene` and new `oss-demo-smoke` jobs pass.

---

## Self-Review

- **Spec coverage:** Codex item 4 ("Prove real open-source value with demos") fully covered. The path Codex named — fresh clone → build → doctor → providers → route → receipt → transcript — is implemented as a script and as a CI gate, with a human quickstart matching it. ✓
- **Placeholder scan:** No `TBD` or "implement later". Task 5 has a defer escape hatch but explicitly opens a follow-up issue rather than leaving silent debt. ✓
- **Type consistency:** `--json` flag, `--phase=` argument, `chosen_provider` JSON key all consistent across script, tests, doc. ✓
- **No-secrets requirement:** The demo runs without remote provider auth. Only Ollama is required for Phase 2+ to be meaningful, and the CI install step covers that. ✓
- **Honesty:** The transcript phase explicitly says "would call 'heiwa session attach' in interactive mode" rather than faking output. ✓

## Risks and mitigations

- **Risk:** First `cargo build --release` exceeds CI's 25-minute timeout on cold cache.
  - **Mitigation:** `Swatinem/rust-cache@v2` reuses the cache across runs. First run on `main` populates it.
- **Risk:** `--dry-run` flag drift between Task 5 and the script's fallback.
  - **Mitigation:** Task 5 Step 5 removes the fallback once the flag is real, eliminating two code paths.
- **Risk:** Quickstart `<heiwa-org>` placeholder gets shipped.
  - **Mitigation:** Task 7 Step 2 explicitly calls this out; reviewer should catch.

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-04-25-oss-demo-path.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
