# Publishing Pipeline

How the `heiwa-universe` repository becomes the public Heiwa surface. GitHub stays authoritative for source, releases, and docs. Cloudflare is DNS utility only; operator evidence remains local.

> Heiwa.ltd delivers software. The operator machine runs the runtime.

## Three publishing planes

| Plane                 | Surface          | Source in repo                                  | Authority       |
| --------------------- | ---------------- | ----------------------------------------------- | --------------- |
| **Marketing shell**   | `heiwa.ltd`      | `apps/heiwa_app/clients/web/`                   | GitHub Pages    |
| **Documentation**     | `docs.heiwa.ltd` | `docs/` + `mkdocs.yml`                          | GitHub Pages    |
| **Releases**          | GitHub Releases  | `apps/heiwa_core/`, `apps/heiwa_shell/`         | GitHub Releases |
| **Evidence + recall** | owner-local      | `crates/heiwa_evidence/`, `crates/heiwa_embed/` | Local JSONL     |

Each plane has a single source of truth in the repo and a single deploy path. Automated workflows are the normal channel; a [manual fallback](#manual-fallback-when-actions-are-paused) exists for the periods when GitHub Actions are paused.

## Public web — GitHub Pages with Cloudflare DNS

`heiwa.ltd` is a static surface. It exists to deliver the installer, marketing copy, install funnel, and support routing. **It does not execute operator work.**

- **Build output**: `apps/heiwa_app/clients/web/` (static HTML + CSS + JS)
- **Host authority**: GitHub Pages
- **Cloudflare role**: DNS records only
- **Routes**: `heiwa.ltd` -> marketing/install/support; `docs.heiwa.ltd` -> documentation. The primary app is HOME-installed at `~/.heiwa/app/Heiwa.app`.

### What Cloudflare must never receive

- Operator state, memory, sessions, or evidence
- Provider secrets, API keys, OAuth tokens
- Local evidence journals or Lance indexes

If a future feature appears to need any of the above on Cloudflare, treat it as a design escape and route through governance before shipping.

## GitHub — the authoritative repository

GitHub is the source of truth. Every public artifact is built from a tagged commit on `main`.

| Workflow                                                                                                | Trigger                         | Output                                         |
| ------------------------------------------------------------------------------------------------------- | ------------------------------- | ---------------------------------------------- |
| [`ci.yml`](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/.github/workflows/ci.yml)           | PRs to `main` + manual dispatch | Rust matrix, lint, docs, agent-sync, hygiene   |
| [`pages.yml`](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/.github/workflows/pages.yml)     | tag push `v*`                   | MkDocs build → GitHub Pages → `docs.heiwa.ltd` |
| [`release.yml`](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/.github/workflows/release.yml) | tag push `v*`                   | Cross-platform binaries → GitHub Releases      |
| [`deploy.yml`](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/.github/workflows/deploy.yml)   | manual dispatch only            | Cloudflare Pages publish for `clients/web/`    |

CI economy: compute runs at the moments that matter — PR validation (the production gate for `main`), tagged releases, and deliberate dispatches. Merges to `main` do not implicitly re-test or republish anything. `bash scripts/check_ci_local.sh` mirrors the PR checks locally and is the required pre-push gate.

### Current pipeline status (2026-07-30)

The Actions lock is **chronic, not incidental**: runner-billed-out since at least 2026-05-25, briefly cleared (PR #51 ran 27 green minutes on 2026-07-28), then re-locked with "recent account payments have failed". The durable fix is downgrading the account to **GitHub Free** (Settings → Billing and licensing → Current plan → Downgrade): this repository is public, and standard GitHub-hosted runners on public repositories bill nothing, so no spending limit or paid plan is required. The workflow code paths remain valid — no code change is needed when runners return.

Until that resolves, ship from the manual fallback below. Remove the fallback section once at least one tagged release flows cleanly through the automated path again — doctrine pages stay honest.

### Manual fallback (when Actions are paused)

**Docs → `docs.heiwa.ltd`**

```bash
uv run --extra docs mkdocs build --strict
uv run --extra docs mkdocs gh-deploy --force
```

`gh-deploy` pushes the built site to the `gh-pages` branch, which GitHub Pages serves. `--force` is appropriate because `gh-pages` is generated state, not source.

**Binary releases → GitHub Releases**

Build each target locally (or in a clean sandbox), assemble the archive, generate the checksums manifest, and create the release with `gh`:

```bash
TAG=v0.1.0
mkdir -p dist

# macOS · Apple Silicon
cargo build --release --target aarch64-apple-darwin -p heiwa-shell
tar -czf dist/heiwa-${TAG}-macos-aarch64.tar.gz \
  -C target/aarch64-apple-darwin/release heiwa

# Linux · x86_64  (cross-build via Docker or build on a Linux host)
cargo build --release --target x86_64-unknown-linux-gnu -p heiwa-shell
tar -czf dist/heiwa-${TAG}-linux-x86_64.tar.gz \
  -C target/x86_64-unknown-linux-gnu/release heiwa

# Windows · x86_64  (cross-build via cargo-xwin or build on a Windows host)
cargo build --release --target x86_64-pc-windows-msvc -p heiwa-shell
zip -j dist/heiwa-${TAG}-windows-x86_64.zip \
  target/x86_64-pc-windows-msvc/release/heiwa.exe

# Checksums manifest — authoritative for mirrors and the installer
( cd dist && shasum -a 256 heiwa-${TAG}-* > heiwa-${TAG}-checksums.txt )

# Cut the release
gh release create ${TAG} dist/heiwa-${TAG}-* \
  --title "Heiwa ${TAG}" \
  --notes-file CHANGELOG.md
```

The result is byte-identical to what `release.yml` produces. The checksums manifest stays authoritative. Run `bash scripts/check_ci_local.sh` before packaging (it mirrors the PR checks exactly), and prefer `bash scripts/package_release_sandbox.sh` for a clean-room build. Uploading locally built artifacts to a GitHub Release does not require Actions.

### macOS distribution without the Apple Developer Program

Heiwa's desktop bundle is **ad-hoc signed** (`signingIdentity: "-"` in
`apps/heiwa_app/desktop/src-tauri/tauri.conf.json`). Apple Developer Program
membership (≈US$99/yr) is required for Developer ID signing and notarization —
not for the legal right to distribute your own software. Ground rules:

- Publish the `.dmg`/`.zip` with SHA-256 checksums, the source tag, license,
  and third-party notices.
- Tell users plainly: macOS will flag the app as from an unverified developer.
  The unblock path is System Settings → Privacy & Security → **Open Anyway**.
- Never describe a build as "Apple verified", "Developer ID signed", or
  "notarized" — none of those are true for ad-hoc signatures. Honesty over
  install friction.
- This route serves technical users and early releases. When consumer-grade
  install friction matters, the paid Apple program becomes unavoidable —
  treat that as a deliberate future decision, not a default.

**Cloudflare Pages → `heiwa.ltd`**

```bash
npx wrangler pages deploy apps/heiwa_app/clients/web --project-name=heiwa-clients
```

Functionally identical to the `deploy.yml` workflow path.

### Release tagging conventions

- Tags follow `v<major>.<minor>.<patch>` (semver). Pre-releases are `vX.Y.Z-rcN`.
- A tag triggers **both** `pages.yml` (docs) and `release.yml` (binaries) in parallel. Order is not enforced — readers can land on either surface independently.
- The release manifest (`heiwa-<version>-checksums.txt`) is the canonical install-time verifier. The installer at `https://heiwa.ltd/install` resolves the latest tag and pulls the matching archive.

### Release artifacts

For each tag the release workflow produces:

```
heiwa-<version>-macos-aarch64.tar.gz
heiwa-<version>-linux-x86_64.tar.gz
heiwa-<version>-windows-x86_64.zip
heiwa-<version>-checksums.txt
```

See [Install Guide](https://heiwa.ltd/download.html) for the operator-facing summary.

## Local evidence and recall

The installed runtime writes canonical, versioned JSONL journals through
`crates/heiwa_evidence/`. Lance tables from `crates/heiwa_embed/` are derived,
rebuildable local recall indexes. Neither is a public publishing surface.

GitHub evidence sync is planned, not active. Any future projection must be
explicitly enabled, redacted before leaving the machine, and incapable of
becoming a second write authority.

## Operator boundary diagram

```
+---------------------------------------------------------------+
|                       PUBLIC BACKBONE                         |
|                                                               |
|   Cloudflare DNS          GitHub Pages + Releases              |
|   (records only)          (site, docs, source, binaries)       |
|                                                               |
+--------------------------------|------------------------------+
                                 | install + identity exchange
                                 v
+---------------------------------------------------------------+
|                       OPERATOR MACHINE                        |
|                                                               |
|   heiwa runtime (Rust)    provider CLIs    local models       |
|   memory, sessions,       OAuth tokens,    Ollama, etc.       |
|   approvals, evidence     API keys                            |
|                                                               |
|   evidence stays local; future sync is opt-in and redacted    |
+---------------------------------------------------------------+
```

## Common operator questions

### Does Heiwa run on Cloudflare?

No. `heiwa.ltd` is a static site delivered by Cloudflare Pages. The runtime, app, memory, and provider secrets all live on the operator machine.

### Why GitHub Pages for docs and not Cloudflare?

Docs are tightly coupled to source — every tag publishes both. GitHub Pages keeps the doc surface authoritative against the commit it was built from. Cloudflare hosts the marketing surface where doc-source coupling is not a requirement.

### Why JSONL plus Lance?

JSONL keeps durable truth inspectable, replayable, and Git-friendly. Lance gives
fast local vector recall without becoming a second authority; the index can be
rebuilt from the text corpus.

### Can I self-host the publishing pipeline?

The repository is the entire surface. Fork it, point a Pages project at `clients/web/`, and run MkDocs against `docs/` to get an isolated mirror. The release workflow is tagged-trigger driven and runs in any GitHub Actions account with no Heiwa-specific secrets beyond release signing.

## Change-control rules

- Cloudflare DNS changes go through the tracked infrastructure path — never through the dashboard.
- New workflows or workflow edits land in `.github/workflows/` with the same review gate as runtime code.
- Evidence envelope or migration changes require compatibility tests and local replay verification.
- The public/runtime boundary is a doctrine line. If a publishing change appears to need operator state on a public surface, route through [governance](support.html#governance--boundaries) before opening the PR.

## Where to next

- [Install Guide](https://heiwa.ltd/download.html) — operator-facing install path
- [Architecture](architecture.md) — full runtime architecture
- [Security](security.md) — disclosure policy and runtime threat model
- [Operator Runbook](operator-runbook.md) — day-to-day operation
