# Heiwa Single Truth

Date: 2026-05-27
Status: operator strategy, grounded in local repo, worktrees, GitHub, runtime probes, and current peer research

## Verdict

Heiwa should ship as a local-first installed runtime plus app. The product is
not the Cloudflare site, not SpacetimeDB, not a hosted Rust backend, and not
Devon's private checkout.

Compression:

> Heiwa.app displays. `heiwa` runs. SQLite/files record local truth. SpacetimeDB
> syncs narrow adjudication/evidence when enabled. GitHub releases distribute.
> Cloudflare fronts public-safe pages and manifests.

## Current Truth

- Repository: `Strategizing/heiwa-universe`, private on GitHub as of this check.
- Default branch: `main`.
- Local checkout: `/Users/dmcgregsauce/heiwa-universe`, currently dirty with
  peer-agent work.
- Worktrees:
  - `main` at `8f79e422` / PR #44 merged.
  - `claude/friendly-meninsky-3f68aa` at `bda8aa0`, PR #45 draft, receipts work.
  - two additional Claude worktrees point at current `main`.
- GitHub Releases: none published.
- Git tags exist, including old backup tags and `v1.0`, but no tag should be
  treated as stable distribution until a release artifact and checksum exist.
- Public `https://heiwa.ltd/install` currently serves a source-build installer
  that clones `https://github.com/Strategizing/heiwa-universe.git` from `main`;
  this fails for normal users while the repo is private and is not release
  provenance.
- `docs.heiwa.ltd` returned GitHub Pages 404 during this check.
- `api.heiwa.ltd/status` returned Cloudflare 522 during this check.
- `heiwa app update --dry-run` correctly defaults to GitHub Releases and reports
  `source_mode: github-release`, but release asset verification is not fully
  wired.
- PR #45 is mergeable but its only failing check is GitHub's auto-injected
  `Automatic Dependency Submission (Python)` run. The job has no steps or logs;
  its check-run annotation says it was not started because of recent account
  payments or spending-limit state. Treat it as a GitHub settings/billing
  blocker, not a code-test failure.
- A current tracked-tree `gitleaks dir` scan is clean after replacing
  token-shaped examples and placeholder literals. Full git-history
  `gitleaks detect` still reports 38 redacted findings across old commits; public
  release still needs history rewrite, credential rotation, or an explicit
  baseline decision.

## Answers

### 1. STDB Option C?

Yes. Confirm Option C with sharper wording:

**Local-first, SQLite/file-backed runtime truth; SpacetimeDB opt-in sync and
adjudication.**

STDB should not be the local app database. It is the online coordination and
evidence plane for receipt headers, licence/entitlement state, leases, routing
decisions, and cross-device continuity when configured. Local execution must
work without public DNS, Cloudflare auth, or STDB connectivity.

Use SQLite locally for:

- receipt store: `~/.heiwa/receipts.db`
- quota/rate ledger: `~/.heiwa/state.db`
- session/search/read models where structured local reads beat raw JSON
- Memory Tree or source chunks if/when Heiwa adds inspectable memory

Use files locally for:

- machine identity: `~/.heiwa/machine.json`
- accounts/config: `~/.heiwa/accounts.json`, `~/.heiwa/config.toml`
- approvals, dispatch, workers, traces, readable artifacts

Use STDB for:

- header-only evidence mirror
- reducer-governed state transitions
- cross-device leases and session continuity
- Heiwa Limited licence/subscription truth later

### 2. E2B-only sandbox?

Confirm only for **untrusted external code**.

The rule should be:

> Untrusted code runs in E2B or equivalent disposable sandbox. Trusted local
> repo work may run in the local checkout/worktree under Heiwa leases,
> approvals, receipts, and dirty-tree rules.

Do not force all work through E2B. Heiwa's value is using the user's machine and
provider CLIs safely. Browser probes, local Rust builds, checkout verification,
provider CLI execution, and local model calls are legitimate local runtime work.

### 3. TUI-first intake?

No, not as the product framing.

Use **single-IO first**:

- CLI/TUI for Devon/operator speed and development proof.
- `Heiwa.app` for normal-user install experience.
- The same local read models and event stream power both.

The build order should be:

1. Local read models: Today, Freshness, Inbox, History, Approvals, Receipts.
2. Event stream: runs, tools, workers, approvals, receipts, blockers.
3. Cockpit over those local APIs.
4. Tauri 2 wrapper for macOS.
5. Signed/notarized release artifact attached to GitHub Release.

TUI can exist, but it should not become another product center.

### 4. Any crate that must not be touched in the merge audit?

Do not do a broad crate merge yet.

Protected unless a slice explicitly requires them:

- `crates/heiwa_protocol`
- `crates/heiwa_stdb`
- `crates/heiwa_provider`
- `crates/heiwa_drex`
- `crates/heiwa_install`
- `crates/heiwa_vault`

Audit first, then merge only duplicate or legacy-adjacent mechanics. The current
problem is not just crate count; it is whether each crate owns a clear boundary.
Do not collapse boundaries that map to product contracts: protocol, provider,
quota, vault, install, session, STDB, DREX.

### 5. Is `heiwa_hub` deletable?

Not immediately.

It is not current product spine and should stay out of normal mutation paths,
but deletion should wait until:

1. product-surface audit says no current code imports it;
2. any unique tests, schema notes, or reducer patterns are migrated or marked
   obsolete;
3. public docs no longer reference it as an active surface;
4. a release can be built without it.

For public OSS, `legacy/apps/heiwa_hub/` is acceptable only if clearly labelled
legacy/reference. If the repo needs to look small and clean for first public
release, archive it after the migration proof, not before.

## Peer Lessons To Copy

### OpenHuman

OpenHuman proves users value UI-first onboarding, local readable memory, OAuth
connector breadth, and scheduled ingestion. Its current README also states that
the default managed path uses hosted services for sign-in, model routing, search
proxying, OAuth, and Composio-backed integrations. That means it is not a pure
local-first proof.

Heiwa should copy:

- desktop-first onboarding
- local SQLite plus editable Markdown-style memory/export
- freshness indicators
- connector setup that a normal user can finish
- token/source compression before cloud model calls

Heiwa should beat:

- local side-effect authority
- local provider CLI ownership
- approval packets before risky writes
- GitHub release and checksum provenance
- STDB header-only evidence sync instead of broad backend custody

### Hermes Agent

Hermes proves durable terminal agents, skill learning, FTS5 recall, messaging
gateways, cron, MCP, provider switching, and install/update/doctor surfaces are
now table stakes. It is not proof of a cooperating worker mesh by itself.

Heiwa should copy:

- `doctor` / `update` as serious product surfaces
- skills/procedures as procedural memory
- FTS5/session recall
- gateway intake
- scheduled automations
- MCP as first-class capability transport

Heiwa should beat:

- typed capability leases
- approval-gated side effects
- receipt-backed execution
- provider-owned auth/runtime truth
- local machine/fleet awareness

### OpenHuman Skills / Package Pattern

OpenHuman's separate skills registry is a useful pattern: skills are packageable
units with manifests, setup, lifecycle, hooks, cron, isolated storage, and no
hardcoded credentials.

Heiwa should not copy it blindly. Heiwa should define separate classes:

- provider adapters
- tools
- hooks
- reducers/policies
- skills/procedures

Calling all of these "plugins" would hide trust boundaries.

## Release And Public OSS Rule

Before public launch:

1. Keep repo private until secret history scan, licence audit, runtime artifact
   audit, and release dry-run pass.
2. Cut an immutable semver tag such as `v0.1.0`.
3. Build release artifacts from that tag.
4. Attach archives plus `heiwa-<version>-checksums.txt` to GitHub Releases.
5. Mark one release as GitHub "Latest".
6. Let Cloudflare serve install pages and a manifest that points back to the
   immutable GitHub release tag and checksum.
7. Do not use a mutable `latest` tag as binary identity.

Channel semantics:

- `vX.Y.Z` is immutable binary identity.
- GitHub "Latest" is the default latest release pointer.
- `stable` should be a channel manifest value, not an install trust root.
- Container images may use `latest`; local app installers should verify
  immutable release tag plus checksum.

## Immediate Sequence

1. Merge or explicitly park PR #45. It adds local receipt truth, but the only
   failing check is an auto-injected dependency-submission job that never starts.
2. Clean local `main`: separate peer changes from intended trunk work before any
   public flip.
3. Fix public truth drift:
   - no "releases available" claim until a release exists;
   - `docs.heiwa.ltd` must not 404;
   - `api.heiwa.ltd/status` must not be cited while returning 522;
   - `/install` must either require authenticated private-source install or
     point to a public release asset.
4. Finish local app database/read-model spine:
   - SQLite for receipts/quota/session search;
   - files for human-readable machine/account/approval state;
   - STDB mirror only when configured.
5. Add Tauri 2 macOS wrapper only after the local API/read models are stable.
6. Run full public gate:
   - secret scan across history;
   - current tracked-tree secret scan;
   - release metadata check;
   - licence audit;
   - tracked runtime artifact audit;
   - sandbox release package check;
   - install from release in a clean temp home.

## One Product Sentence

Heiwa is the local-first operating layer that turns one human intent into
governed, routed, verified multi-tool AI execution across the user's machine,
providers, local models, and optional cloud sync.
