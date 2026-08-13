# Execution Backlog

Single ordered list of unfinished work. Ideation is frozen: nothing enters this
file that is not a step toward `decision → implementation → tests → installed
runtime → release → public install → receipt`.

**Verified:** 2026-08-12 against the live repo, the installed runtime at
`~/.heiwa`, GitHub (`Heiwa-Limited/heiwa-universe`), and `https://heiwa.ltd`.
Nothing below is carried over from an earlier plan; each line has evidence.

Only the last stage counts. A merged branch is not a release. A green test is
not an install.

## Chain state

| Stage             | State  | Evidence                                                                                      |
| ----------------- | ------ | --------------------------------------------------------------------------------------------- |
| decision          | over   | 48 files in `docs/`, 15 at repo root. Supply exceeds demand.                                    |
| implementation    | green  | `cargo build --workspace` clean. 1 `TODO` marker in all of `apps/` + `crates/`.                 |
| tests             | green  | 297 tests pass in `heiwa-shell` alone. **Never verified in CI** for anything after 2026-07-30. |
| installed runtime | stale  | `~/.heiwa/bin/heiwa` built 2026-08-01. `dev` HEAD is 2026-08-12.                                |
| release           | zero   | `gh release list` is empty. `release.yml` has never executed.                                   |
| public install    | closed | Repo is **private**. `heiwa.ltd/install` hard-exits: "the public release installer is not live yet." |
| receipt           | none   | Nothing has completed the chain, so there is nothing to receipt.                                |

The gap is not implementation. Implementation is the healthiest stage. The gap
is everything downstream of `tests`.

## Root cause

`main` has not moved since 2026-07-28. Everything downstream is gated on it:
no `main` advance → no `v*` tag → `release.yml` never fires → no assets → the
installer's own guard stays correct in refusing to run.

PR #52 (`dev` → `main`, 87 files, +2492/−3013) has been open since 2026-07-30
with all nine CI jobs red. Those jobs failed with **zero steps executed**
(started 08:29:15, failed 08:29:18) — the jobs never started; that is not a
test failure. The fix for that condition is commit `ab310836`, *inside PR #52*.
CI has since been proven working: PR #54 ran "Heiwa CI" to success in 7m3s on
2026-08-08. **The checks on #52 are stale, not failing.** They have never been
re-run.

## Backlog

### Done 2026-08-12

- [x] **B0 — Land the stranded `life social` work.** 744 uncommitted lines
      (closed-schema `life_social_v1` projection + route + 20 tests + contract
      doc) were sitting in the working tree. Committed as `1d4e53bd`.
- [x] **B1 — Restore the baseline gate.** `.gitleaksignore` (added 2026-08-03)
      was never classified in `PRODUCT_SURFACE.md`; the surface audit is a
      longest-prefix classifier over `git ls-files`, so a new top-level dotfile
      silently fails the gate. Red since 2026-08-03, unnoticed because CI never
      ran on `dev`. Fixed in `0e99edac`.
      `scripts/check_agent_baseline.sh` now passes.
- [x] **B2 — Fix the untried release build.** `release.yml` pinned Rust 1.93.1
      against a repo `rust-toolchain.toml` of 1.95.0; the action attaches the
      cross-compile target to 1.93.1, then the toolchain file redirects cargo to
      1.95.0, for which that target was never installed. Never observed because
      the workflow has never run. Fixed in `0e99edac`.

### Foundation — must be green before promotion

- [ ] **B3 — Push `dev`.** Local `dev` is **11 commits ahead of `origin/dev`**.
      The 2026-08-01 auth, install-atomicity, heartbeat-pruning and
      secret-loading fixes have never left this machine. They exist in exactly
      one place.
- [ ] **B4 — Re-run CI on PR #52 and read the result.** First honest signal in
      two weeks. Do not merge on stale green.
- [ ] **B5 — Resolve PR #53 (Blacksmith runners) and #54 (Greptile).** Both are
      open CI-infrastructure changes competing with the `ab310836` zero-cost
      economy already in #52. Pick one runner story before merging #52, or they
      will conflict at the workflow level.
- [ ] **B6 — Fix the queued-forever Dependabot runs on `main`.** Runs
      `31552660223`, `31539043322` and others sit queued ~24h, then cancel.
      Separate from the #52 CI story: these are on `main`, which does not yet
      carry the trigger fix.

### Promotion

- [ ] **B7 — Merge #52 to `main`.** The first `main` advance since 2026-07-28.
- [ ] **B8 — Rebuild and reinstall the runtime from merged `main`.** The
      installed binary is currently 11 days and 13 commits behind. A reachable
      port on 7474 is not evidence the changed checkout is running.
- [ ] **B9 — Verify the installed runtime.** `heiwa doctor` on the freshly
      promoted binary, including the new `life social` route.

### Release

- [ ] **B10 — Tag `v0.1.0` on merged `main`.** The existing `v1.0` tag points at
      `1a6abe3a`, which is not the current product and never produced a release.
      Decide whether it is retired or archived — do not release over it.
- [ ] **B11 — Let `release.yml` run for the first time.** Three platform
      archives, a checksum manifest, a GitHub Release, and a GHCR image. Treat
      the first run as an experiment: nothing in this workflow has ever
      executed, so expect failures beyond the toolchain pin fixed in B2.

### Public install

- [ ] **B12 — Decide repo visibility.** `HEIWA.md` and `CLAUDE.md` describe
      "the open-source repo `Strategizing/heiwa-universe`". Live truth is
      **private**, under `Heiwa-Limited`. Both the owner and the visibility are
      stale in the docs. Public install is blocked on this, not on code.
- [ ] **B13 — Replace the source installer at `heiwa.ltd/install`.** It
      currently clones and `cargo install`s from a private repo, and correctly
      refuses to run without `HEIWA_PRIVATE_TOKEN`. It should resolve the latest
      release tag and verify against the checksum manifest from B11.
- [ ] **B14 — Prove the install from a clean machine.** Not from this one.
      Until a machine that has never built Heiwa can install and run it, public
      install is unproven.
- [ ] **B15 — Fix the GitHub Pages 404.** `strategizing.github.io/heiwa-universe/`
      returns 404 while `pages.yml` is tag-triggered, so it has never published.
      Gated on B10 and on the ownership question in B12.

## Carried notes

- `release.yml` interpolates `${{ inputs.tag }}` directly into a `run:` block.
  Dispatch-only and requires repo write, so it is low severity, but it should
  move to `env:` before the workflow is used regularly.
- Branch sprawl is not a problem: 3 local branches, 4 remote. The earlier
  concern about scattered partial branches does not match live state.
- The codebase is not the bottleneck and should not be treated as one.
