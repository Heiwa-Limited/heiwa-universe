# Heiwa Runtime-Owned Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make installed `heiwa` the real runtime authority: `heiwa install` creates and repairs `~/.heiwa/`, migrates old flat-root state into structured runtime paths, seeds native modes/capabilities there, generates provider projections there, and leaves repo-local provider configs as project overlays instead of product authority.

**Architecture:** Create one shared runtime-path contract, then make install/provider/STDB/shell code all use it. Phase 1 stays honest: Heiwa owns `~/.heiwa/`; providers still own auth/session internals; repo-local `.codex/`, `.claude/`, and `.gemini/` stay project overlays; generated provider projections live under `~/.heiwa/generated/` first, with outward sync deferred until provider contracts are stable.

**Tech Stack:** Rust workspace crate for shared runtime paths, Rust installer + migration code, serde/serde_json, string-templated TOML/JSON, Bash installer payload, cargo test

**Spec:** `docs/superpowers/specs/2026-04-10-heiwa-runtime-owned-harness-design.md`

---

## Scope Lock

- Phase 1 is about runtime authority, migration, seed content, projections, and docs.
- Phase 1 does **not** pretend provider-home sync is complete if it is not.
- Phase 1 does **not** invent a full new authored-capability package tree. Use existing repo assets where they already exist, especially `packages/heiwa_skills/heiwa-concise-mode/MODE.md`.
- Phase 1 does **not** delete legacy flat-root files. Copy or migrate-forward safely, then prefer new paths.

---

## File Map

### New files

- `crates/heiwa_paths/Cargo.toml`
- `crates/heiwa_paths/src/lib.rs`
- `crates/heiwa_install/src/runtime_seed.rs`
- `crates/heiwa_install/src/runtime_projection.rs`
- `crates/heiwa_install/tests/runtime_owned_install.rs`
- `crates/heiwa_provider/tests/runtime_paths.rs`
- `apps/heiwa_cli/scripts/install_heiwa.sh`
- `docs/runtime-owned-heiwa.md`

### Modified files

- `Cargo.toml`
- `crates/heiwa_install/Cargo.toml`
- `crates/heiwa_install/src/lib.rs`
- `crates/heiwa_install/tests/install_doctor.rs`
- `crates/heiwa_provider/Cargo.toml`
- `crates/heiwa_provider/src/lib.rs`
- `crates/heiwa_provider/src/registry.rs`
- `crates/heiwa_provider/src/detect/mod.rs`
- `crates/heiwa_provider/tests/provider_auth.rs`
- `crates/heiwa_stdb/Cargo.toml`
- `crates/heiwa_stdb/src/lib.rs`
- `apps/heiwa_shell/Cargo.toml`
- `apps/heiwa_shell/src/main.rs`
- `apps/heiwa_shell/tests/smoke.rs`
- `apps/heiwa_cli/scripts/bootstrap_env.py`
- `README.md`
- `HEIWA.md`
- `.codex/config.toml`
- `.claude/settings.json`
- `.gemini/settings.json`
- `docs/standards/agent_standard_v1.md`

### Runtime outputs to verify

- `~/.heiwa/config.toml`
- `~/.heiwa/machine.json`
- `~/.heiwa/providers/registry.json`
- `~/.heiwa/providers/legacy_connections.json`
- `~/.heiwa/state/identity.json`
- `~/.heiwa/state/connection.json`
- `~/.heiwa/models/inventory.json`
- `~/.heiwa/policies/runtime.toml`
- `~/.heiwa/modes/concise/MODE.md`
- `~/.heiwa/capabilities/research/manifest.json`
- `~/.heiwa/capabilities/operator/manifest.json`
- `~/.heiwa/generated/codex/config.toml`
- `~/.heiwa/generated/claude/settings.json`
- `~/.heiwa/generated/gemini/settings.json`
- `~/.heiwa/generated/antigravity/settings.json`

### Flat-root legacy inputs to migrate forward

- `~/.heiwa/accounts.json`
- `~/.heiwa/provider_connections.json`
- `~/.heiwa/identity.json`
- `~/.heiwa/connection.json`

---

## Task 1: Create One Shared Runtime Path Contract

**Files:**
- Create: `crates/heiwa_paths/Cargo.toml`
- Create: `crates/heiwa_paths/src/lib.rs`
- Modify: `Cargo.toml`
- Modify: `crates/heiwa_install/Cargo.toml`
- Modify: `crates/heiwa_provider/Cargo.toml`
- Modify: `crates/heiwa_stdb/Cargo.toml`
- Modify: `apps/heiwa_shell/Cargo.toml`

- [ ] **Step 1: Add new workspace crate**

Create `crates/heiwa_paths/Cargo.toml` and add `crates/heiwa_paths` to workspace members in root `Cargo.toml`.

- [ ] **Step 2: Implement canonical path helpers**

Create `crates/heiwa_paths/src/lib.rs` with one small authority type:

```rust
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    root: PathBuf,
}

impl RuntimePaths {
    pub fn discover() -> Self {
        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .expect("HOME or USERPROFILE must be set");
        Self::from_home(PathBuf::from(home))
    }

    pub fn from_home(home: PathBuf) -> Self {
        Self { root: home.join(".heiwa") }
    }

    pub fn root(&self) -> &Path { &self.root }
    pub fn config(&self) -> PathBuf { self.root.join("config.toml") }
    pub fn machine(&self) -> PathBuf { self.root.join("machine.json") }
    pub fn provider_registry(&self) -> PathBuf { self.root.join("providers/registry.json") }
    pub fn legacy_connections(&self) -> PathBuf { self.root.join("providers/legacy_connections.json") }
    pub fn identity(&self) -> PathBuf { self.root.join("state/identity.json") }
    pub fn connection(&self) -> PathBuf { self.root.join("state/connection.json") }
    pub fn inventory(&self) -> PathBuf { self.root.join("models/inventory.json") }
    pub fn runtime_policy(&self) -> PathBuf { self.root.join("policies/runtime.toml") }
    pub fn concise_mode(&self) -> PathBuf { self.root.join("modes/concise/MODE.md") }
}
```

- [ ] **Step 3: Add path tests before wiring consumers**

Add unit tests in `crates/heiwa_paths/src/lib.rs` for:

- `RuntimePaths::from_home(PathBuf::from("/tmp/x")).root() == /tmp/x/.heiwa`
- provider registry path is `.../.heiwa/providers/registry.json`
- identity path is `.../.heiwa/state/identity.json`
- connection path is `.../.heiwa/state/connection.json`

- [ ] **Step 4: Run new crate tests**

Run:

```bash
cargo test -p heiwa-paths -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/heiwa_paths/Cargo.toml crates/heiwa_paths/src/lib.rs crates/heiwa_install/Cargo.toml crates/heiwa_provider/Cargo.toml crates/heiwa_stdb/Cargo.toml apps/heiwa_shell/Cargo.toml
git commit -m "feat: add shared heiwa runtime path contract"
```

---

## Task 2: Make `heiwa install` Create Structured Runtime Root and Safe Migration

**Files:**
- Modify: `crates/heiwa_install/src/lib.rs`
- Modify: `crates/heiwa_install/tests/install_doctor.rs`
- Create: `crates/heiwa_install/tests/runtime_owned_install.rs`

- [ ] **Step 1: Add failing install test for structured runtime**

Create `crates/heiwa_install/tests/runtime_owned_install.rs` with temp-home coverage for `run_install()`:

```rust
for dirname in [
    "bin",
    "logs",
    "sessions",
    "cache",
    "state",
    "secrets",
    "providers",
    "models",
    "capabilities",
    "modes",
    "policies",
    "generated",
    "artifacts",
] {
    assert!(runtime_root.join(dirname).is_dir(), "missing {}", dirname);
}

assert!(runtime_root.join("config.toml").exists());
assert!(runtime_root.join("machine.json").exists());
```

- [ ] **Step 2: Add failing migration coverage**

In same test file, pre-create:

- `~/.heiwa/accounts.json`
- `~/.heiwa/provider_connections.json`
- `~/.heiwa/identity.json`
- `~/.heiwa/connection.json`

Then assert after install:

```rust
assert!(runtime_root.join("providers/registry.json").exists());
assert!(runtime_root.join("providers/legacy_connections.json").exists());
assert!(runtime_root.join("state/identity.json").exists());
assert!(runtime_root.join("state/connection.json").exists());
```

Legacy files may remain. New files must exist and contain migrated content.

- [ ] **Step 3: Run tests to verify RED**

Run:

```bash
cargo test -p heiwa-install --test runtime_owned_install -- --nocapture
```

Expected: FAIL because install still only creates flat runtime dirs.

- [ ] **Step 4: Refactor installer to use `heiwa_paths`**

In `crates/heiwa_install/src/lib.rs`:

- replace `get_heiwa_dir()` internals with `heiwa_paths::RuntimePaths::discover().root().to_path_buf()`
- create all structured runtime dirs
- write `config.toml` if absent
- keep writing `machine.json`
- keep writing canonical launcher to `~/.heiwa/bin/heiwa`

Use string templates for TOML. Do **not** add a TOML serialization dependency for this pass.

- [ ] **Step 5: Add safe migrate-forward helper**

Add helper in `crates/heiwa_install/src/lib.rs`:

```rust
fn migrate_if_missing(old: &Path, new: &Path) -> Result<()> {
    if new.exists() || !old.exists() {
        return Ok(());
    }
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(old, new)?;
    Ok(())
}
```

Use it for old flat-root files. Do not remove old files yet.

- [ ] **Step 6: Re-run install tests**

Run:

```bash
cargo test -p heiwa-install --test install_doctor -- --nocapture
cargo test -p heiwa-install --test runtime_owned_install -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/heiwa_install/src/lib.rs crates/heiwa_install/tests/install_doctor.rs crates/heiwa_install/tests/runtime_owned_install.rs
git commit -m "feat: make heiwa install own structured runtime root"
```

---

## Task 3: Seed Native Modes, Capabilities, Inventory, and Policy

**Files:**
- Create: `crates/heiwa_install/src/runtime_seed.rs`
- Modify: `crates/heiwa_install/src/lib.rs`
- Modify: `crates/heiwa_install/tests/runtime_owned_install.rs`

- [ ] **Step 1: Add failing seed assertions**

Extend `runtime_owned_install.rs`:

```rust
assert!(runtime_root.join("modes/concise/MODE.md").exists());
assert!(runtime_root.join("capabilities/research/manifest.json").exists());
assert!(runtime_root.join("capabilities/operator/manifest.json").exists());
assert!(runtime_root.join("models/inventory.json").exists());
assert!(runtime_root.join("policies/runtime.toml").exists());
```

- [ ] **Step 2: Run test to verify RED**

Run:

```bash
cargo test -p heiwa-install --test runtime_owned_install -- --nocapture
```

Expected: FAIL because runtime seed content does not exist yet.

- [ ] **Step 3: Add runtime seed module**

Create `crates/heiwa_install/src/runtime_seed.rs` with:

- `seed_concise_mode(root, repo_root)` reading from `packages/heiwa_skills/heiwa-concise-mode/MODE.md`
- `seed_capability_manifest(root, "research", ...)`
- `seed_capability_manifest(root, "operator", ...)`
- blank-but-valid `models/inventory.json`
- minimal `policies/runtime.toml`

Important: if canonical repo asset missing, fail loudly. Do not silently fall back to fake placeholder text for concise mode.

- [ ] **Step 4: Wire seeding into install**

In `crates/heiwa_install/src/lib.rs` call:

```rust
runtime_seed::seed_runtime(&paths, &get_repo_root())?;
```

- [ ] **Step 5: Re-run install tests**

Run:

```bash
cargo test -p heiwa-install --test runtime_owned_install -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/heiwa_install/src/lib.rs crates/heiwa_install/src/runtime_seed.rs crates/heiwa_install/tests/runtime_owned_install.rs
git commit -m "feat: seed native heiwa modes and capabilities"
```

---

## Task 4: Move Provider and STDB State to Structured Runtime Paths With Fallback

**Files:**
- Modify: `crates/heiwa_provider/src/lib.rs`
- Modify: `crates/heiwa_provider/src/registry.rs`
- Modify: `crates/heiwa_provider/src/detect/mod.rs`
- Modify: `crates/heiwa_provider/tests/provider_auth.rs`
- Create: `crates/heiwa_provider/tests/runtime_paths.rs`
- Modify: `crates/heiwa_stdb/src/lib.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `apps/heiwa_cli/scripts/bootstrap_env.py`

- [ ] **Step 1: Add failing provider-path tests**

Create `crates/heiwa_provider/tests/runtime_paths.rs` with temp-home coverage:

- `AccountRegistry::save()` writes to `~/.heiwa/providers/registry.json`
- identity saves to `~/.heiwa/state/identity.json`
- legacy flat files still load when new files absent

Example assertion:

```rust
assert!(saved_path.ends_with(".heiwa/providers/registry.json"));
```

- [ ] **Step 2: Add failing STDB fallback test or assertion coverage**

Either:

- add unit tests in `crates/heiwa_stdb/src/lib.rs`, or
- add a small integration test under `apps/heiwa_shell/tests/`

Coverage required:

- `StdbConfig::resolve()` prefers `state/connection.json`
- falls back to flat `connection.json` for one release
- uses `state/identity.json` as new presence gate

- [ ] **Step 3: Run tests to verify RED**

Run:

```bash
cargo test -p heiwa-provider -- --nocapture
cargo test -p heiwa-stdb -- --nocapture
```

Expected: FAIL on old flat-path assumptions.

- [ ] **Step 4: Switch provider code to `heiwa_paths`**

Change:

- `accounts.json` -> `providers/registry.json`
- `provider_connections.json` -> `providers/legacy_connections.json`
- `identity.json` -> `state/identity.json`

Behavior rule:

- loaders check new path first
- if new path missing, read old flat file
- savers always write new path

- [ ] **Step 5: Switch STDB + shell login to `state/connection.json`**

Update:

- `crates/heiwa_stdb/src/lib.rs`
- `apps/heiwa_shell/src/main.rs` `login` branch
- `apps/heiwa_cli/scripts/bootstrap_env.py`

New rule: all new writes go to structured runtime paths. Old flat file only exists as legacy input.

- [ ] **Step 6: Re-run provider and STDB tests**

Run:

```bash
cargo test -p heiwa-provider -- --nocapture
cargo test -p heiwa-stdb -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/heiwa_provider/src/lib.rs crates/heiwa_provider/src/registry.rs crates/heiwa_provider/src/detect/mod.rs crates/heiwa_provider/tests/provider_auth.rs crates/heiwa_provider/tests/runtime_paths.rs crates/heiwa_stdb/src/lib.rs apps/heiwa_shell/src/main.rs apps/heiwa_cli/scripts/bootstrap_env.py
git commit -m "feat: move provider and stdb state under structured heiwa runtime"
```

---

## Task 5: Generate Provider Projections and Expose Runtime-Owned CLI UX

**Files:**
- Create: `crates/heiwa_install/src/runtime_projection.rs`
- Modify: `crates/heiwa_install/src/lib.rs`
- Modify: `crates/heiwa_install/tests/runtime_owned_install.rs`
- Modify: `apps/heiwa_shell/src/main.rs`
- Modify: `apps/heiwa_shell/tests/smoke.rs`

- [ ] **Step 1: Add failing projection assertions**

Extend `runtime_owned_install.rs`:

```rust
assert!(runtime_root.join("generated/codex/config.toml").exists());
assert!(runtime_root.join("generated/claude/settings.json").exists());
assert!(runtime_root.join("generated/gemini/settings.json").exists());
assert!(runtime_root.join("generated/antigravity/settings.json").exists());
```

- [ ] **Step 2: Add failing CLI smoke expectations**

Extend `apps/heiwa_shell/tests/smoke.rs` to cover:

- `heiwa doctor` prints runtime root
- `heiwa doctor` prints concise mode presence
- `heiwa repair` exists and succeeds

Example assertions:

```rust
assert!(stdout.contains(".heiwa"));
assert!(stdout.contains("concise"));
```

- [ ] **Step 3: Run tests to verify RED**

Run:

```bash
cargo test -p heiwa-install --test runtime_owned_install -- --nocapture
cargo test -p heiwa-shell --test smoke -- --nocapture
```

Expected: FAIL.

- [ ] **Step 4: Implement projection generator**

Create `crates/heiwa_install/src/runtime_projection.rs` to write minimal generated views under `~/.heiwa/generated/`:

- Codex: terse config overlay or projection note
- Claude: generated settings JSON
- Gemini: generated settings JSON
- Antigravity: generated settings JSON derived from Gemini shape

Keep these files clearly marked generated in content where format allows. For JSON, use a top-level `_generated_by` field only if consumer tolerates unknown keys; otherwise keep marker in adjacent docs, not in machine-read config.

- [ ] **Step 5: Add `repair` and richer `doctor` UX**

In `apps/heiwa_shell/src/main.rs`:

- add `repair` command that reruns install, seed, migration, and projection generation
- make `doctor` print runtime root, registry path, concise mode status, and generated projection status

Do **not** claim provider-home sync is complete unless actual sync exists.

- [ ] **Step 6: Re-run tests**

Run:

```bash
cargo test -p heiwa-install --test runtime_owned_install -- --nocapture
cargo test -p heiwa-shell --test smoke -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/heiwa_install/src/lib.rs crates/heiwa_install/src/runtime_projection.rs crates/heiwa_install/tests/runtime_owned_install.rs apps/heiwa_shell/src/main.rs apps/heiwa_shell/tests/smoke.rs
git commit -m "feat: generate provider projections and add heiwa repair"
```

---

## Task 6: Add Honest Hosted Installer Payload

**Files:**
- Create: `apps/heiwa_cli/scripts/install_heiwa.sh`
- Modify: `README.md`
- Modify: `docs/runtime-owned-heiwa.md`

- [ ] **Step 1: Add installer payload script**

Create `apps/heiwa_cli/scripts/install_heiwa.sh` as canonical hosted payload:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="${HEIWA_ROOT:-$HOME/.heiwa}"
mkdir -p "$ROOT/bin"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo required for source install in this phase" >&2
  exit 1
fi

cargo install --path apps/heiwa_shell --root "$ROOT"
"$ROOT/bin/heiwa" install
```

This is phase-1 honest behavior. No fake binary CDN story.

- [ ] **Step 2: Add shell syntax verification**

Run:

```bash
bash -n apps/heiwa_cli/scripts/install_heiwa.sh
```

Expected: exit 0.

- [ ] **Step 3: Update README honestly**

Change `README.md` Quick Start to:

- lead with installed `heiwa`
- show repo-source install that works now
- mention hosted `curl ... | sh` only as deployment target if script is actually published

Remove or demote badges/claims that make Railway or hosted web app look like product center.

- [ ] **Step 4: Document runtime-owned contract**

Create `docs/runtime-owned-heiwa.md` covering:

- `~/.heiwa/` as authority
- structured path contract
- safe legacy migration
- generated provider projections under `~/.heiwa/generated/`
- repo-local provider configs are overlays, not canonical authority

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_cli/scripts/install_heiwa.sh README.md docs/runtime-owned-heiwa.md
git commit -m "feat: add hosted installer payload and runtime docs"
```

---

## Task 7: Shrink Repo-Local Configs to Overlay Status and Clean Public Story

**Files:**
- Modify: `.codex/config.toml`
- Modify: `.claude/settings.json`
- Modify: `.gemini/settings.json`
- Modify: `HEIWA.md`
- Modify: `docs/standards/agent_standard_v1.md`

- [ ] **Step 1: Trim repo-local configs to project-only posture**

Rules:

- `.codex/config.toml` may keep comment header because TOML supports comments
- `.claude/settings.json` and `.gemini/settings.json` must stay valid JSON; no comment hacks
- remove or keep only settings that are truly repo-scoped
- do not try to encode canonical runtime authority inside provider JSON if schema support is unclear

- [ ] **Step 2: Update Codex overlay header**

Make `.codex/config.toml` say clearly:

```toml
# Heiwa repo overlay only.
# Installed runtime authority lives under ~/.heiwa/.
# Keep this file project-scoped and minimal.
```

- [ ] **Step 3: Update docs, not JSON, for overlay truth**

In `HEIWA.md` and `docs/standards/agent_standard_v1.md`, add explicit language:

- harness first
- `heiwa` owns runtime authority
- repo-local provider files are overlays
- GitHub should foreground OSS product surfaces, not support infra

- [ ] **Step 4: Validate config syntax**

Run:

```bash
python3 - <<'PY'
import json, tomllib
tomllib.load(open('.codex/config.toml','rb'))
json.load(open('.claude/settings.json'))
json.load(open('.gemini/settings.json'))
print('ok')
PY
```

Expected: `ok`

- [ ] **Step 5: Commit**

```bash
git add .codex/config.toml .claude/settings.json .gemini/settings.json HEIWA.md docs/standards/agent_standard_v1.md
git commit -m "docs: make heiwa runtime authority and overlay boundaries explicit"
```

---

## Final Verification

- [ ] **Step 1: Run path and install tests**

```bash
cargo test -p heiwa-paths -- --nocapture
cargo test -p heiwa-install --test install_doctor -- --nocapture
cargo test -p heiwa-install --test runtime_owned_install -- --nocapture
```

- [ ] **Step 2: Run provider and STDB tests**

```bash
cargo test -p heiwa-provider -- --nocapture
cargo test -p heiwa-stdb -- --nocapture
```

- [ ] **Step 3: Run shell smoke tests**

```bash
cargo test -p heiwa-shell --test smoke -- --nocapture
```

- [ ] **Step 4: Verify installer payload and config syntax**

```bash
bash -n apps/heiwa_cli/scripts/install_heiwa.sh
python3 - <<'PY'
import json, tomllib
tomllib.load(open('.codex/config.toml','rb'))
json.load(open('.claude/settings.json'))
json.load(open('.gemini/settings.json'))
print('config ok')
PY
```

- [ ] **Step 5: Manual runtime sanity check**

```bash
~/.heiwa/bin/heiwa install
~/.heiwa/bin/heiwa doctor
~/.heiwa/bin/heiwa providers
```

Expected:

- install creates structured runtime tree
- doctor reports runtime root and generated projections honestly
- providers still show wrapped-provider reality honestly

---

## Guardrails

- No maturity theater. If hosted installer URL is not live, do not pretend it is.
- No destructive migration of old flat-root files in this phase.
- No fake “native provider parity” language for Codex, Gemini, or Antigravity.
- No new product noun for “apps” or “skills” inside runtime. Internal noun is **capability**.
- Keep concise/Caveman as native Heiwa mode, not plugin-led product structure.
- Keep provider auth/session semantics provider-owned.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-10-heiwa-runtime-owned-harness.md`. Two execution options:

**1. Subagent-Driven (recommended)** - dispatch a fresh subagent per task, review between tasks

**2. Inline Execution** - execute tasks in this session using executing-plans

Which approach?
