# Publishing Pipeline

How the `heiwa-universe` repository becomes the public Heiwa surface. GitHub owns source, release binaries, checksums, and provenance; GitHub Pages hosts the docs. Cloudflare serves DNS and the static public shell/installer. Runtime authority and private evidence remain local.

> Heiwa.ltd delivers software. The operator machine runs the runtime.

## Publishing surfaces and local evidence

| Plane                 | Surface          | Source in repo                                  | Authority       |
| --------------------- | ---------------- | ----------------------------------------------- | --------------- |
| **Marketing shell**   | `heiwa.ltd`      | allowlisted package from `apps/heiwa_app/clients/web/` | Cloudflare Pages |
| **Documentation**     | `docs.heiwa.ltd` | `docs/` + `mkdocs.yml`                          | GitHub Pages    |
| **Releases**          | GitHub Releases  | `apps/heiwa_core/`, `apps/heiwa_shell/`         | GitHub Releases |
| **Evidence + recall** | owner-local      | `crates/heiwa_evidence/`, `crates/heiwa_embed/` | Local JSONL     |

Public artifacts come from repository source through the workflows below. Local evidence is a separate runtime surface and is never part of a public deployment. [Verification and dispatch](#verification-and-dispatch) distinguish local build receipts from publication.

## Public web — Cloudflare Pages

`heiwa.ltd` is a static surface. It exists to deliver the installer, marketing copy, install funnel, and support routing. **It does not execute operator work.**

- **Build output**: `.artifacts/public-web`, produced by `bash scripts/package_public_web.sh .artifacts/public-web`
- **Host**: Cloudflare Pages project `heiwa-clients`
- **Cloudflare role**: DNS and static shell/installer delivery; GitHub Releases remains the binary authority
- **Routes**: `heiwa.ltd` -> marketing/install/support; `docs.heiwa.ltd` -> documentation. On macOS the public installer places `Heiwa.app` in `/Applications` when writable, otherwise `~/Applications`; the runtime remains local.

### What Cloudflare must never receive

- Operator state, memory, sessions, or evidence
- Provider secrets, API keys, OAuth tokens
- Local evidence journals or Lance indexes

If a future feature appears to need any of the above on Cloudflare, treat it as a design escape and route through governance before shipping.

## GitHub — the authoritative repository

GitHub is the source authority. Binary releases require an existing annotated tag whose resolved commit belongs to `main`. Docs publish through a manual dispatch from `main`, matching the GitHub Pages environment policy. The public shell deploys from the manually selected ref; use reviewed `main` for production.

| Workflow | Trigger | Output |
| -------- | ------- | ------ |
| [`ci.yml`](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/.github/workflows/ci.yml) | PRs targeting `dev`/`main`, `main` push, manual | Rust tests/static checks, Python tests, security, lint, docs, repository contracts |
| [`certification.yml`](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/.github/workflows/certification.yml) | `main` push, manual | Cross-platform Rust compilation, desktop shell, Lance, multi-ecosystem security proofs |
| [`pages.yml`](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/.github/workflows/pages.yml) | Manual dispatch from `main` | Locked strict MkDocs build → GitHub Pages → `docs.heiwa.ltd` |
| [`release.yml`](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/.github/workflows/release.yml) | Manual dispatch with an existing annotated tag | Certified CLI archives and signed macOS updater bundle → GitHub Releases |
| [`deploy.yml`](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/.github/workflows/deploy.yml) | Manual dispatch only | Allowlisted public artifact → Cloudflare Pages → served installer verification |
| [`container.yml`](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/.github/workflows/container.yml) | Manual dispatch or call from `release.yml` after publication | Verified Linux release bytes → GHCR image with provenance and SBOM |
| [`public-install-smoke.yml`](https://github.com/Heiwa-Limited/heiwa-universe/blob/main/.github/workflows/public-install-smoke.yml) | Manual dispatch or call from `release.yml` after publication | Linux/macOS public installation and runtime checks |

PR feedback jobs have one-minute deadlines; the Rust compile lanes have
20-minute deadlines. Every protected `main` advance runs CI and the separate
certification workflow. Release metadata requires successful `push` runs of
both workflows at the exact resolved release commit. A manual certification
run or a green run at another commit does not satisfy that query.

Run `HEIWA_BRANCH_MODE=experimental bash scripts/check_ci_local.sh` before
promoting an experimental branch into `dev`; run `bash scripts/check_ci_local.sh`
on `dev` before production promotion. Local success does not establish a
remote release or an installed-runtime receipt.

### Verification and dispatch

**Docs → `docs.heiwa.ltd`**

The workflow installs `uv==0.12.1` and builds the locked docs environment:

```bash
uv run --locked --extra docs python -m mkdocs build --strict
gh workflow run pages.yml --ref main
```

`pages.yml` uploads `site/` with `actions/upload-pages-artifact` and deploys that
artifact with `actions/deploy-pages`. The `github-pages` environment permits
`main` only; the build job skips other refs. It does not publish a `gh-pages` branch;
`mkdocs gh-deploy --force` is not the configured deployment path. During an
Actions outage, a local build can validate docs but cannot prove deployment.

**Binary releases → GitHub Releases**

For local packaging verification:

```bash
bash scripts/package_release_sandbox.sh
```

The sandbox creates a host-platform CLI archive, cockpit assets, community
files, and checksums. Its default version is `dev-<git-sha>`; it uploads nothing
and does not replace the cross-platform release, signed desktop bundle, or
GitHub provenance. Publish with the release workflow after completing the
[release sequence](#release-tagging-and-sequence) below.

### macOS release packaging

The desktop configuration uses ad-hoc macOS code signing
(`signingIdentity: "-"`). The release workflow separately signs the updater
archive using `TAURI_SIGNING_PRIVATE_KEY` and requires both the archive and its
signature before publication. An updater signature is not Apple notarization
or Developer ID signing.

The shipped desktop payload is the macOS ARM64 app tarball consumed by the
installer and updater, with its `.sig` file and `latest.json`. `release.yml`
deliberately omits `.dmg` publication. The standalone desktop-build workflow can
produce additional development bundles; those are not the release artifact
contract.

**Cloudflare Pages → `heiwa.ltd`**

The normal production route is `gh workflow run deploy.yml --ref main`.
Publication requires `ENABLE_CLOUDFLARE_DEPLOY=true` and the configured
Cloudflare credentials; otherwise the deploy step skips or fails as declared
in the workflow. For an explicitly authorized manual static deployment, use the
same allowlisted package and pinned Wrangler version:

```bash
bash scripts/package_public_web.sh .artifacts/public-web
npx wrangler@4.122.0 pages deploy .artifacts/public-web --project-name=heiwa-clients --branch=main
HEIWA_PUBLIC_INSTALLER_ATTEMPTS=10 HEIWA_PUBLIC_INSTALLER_RETRY_DELAY=15 \
  bash scripts/check_public_installer_edge.sh
```

Never deploy the source web directory directly; it also contains operator-only
files. The edge check compares served installer bytes with the checkout and
allows time for propagation.

### Release tagging and sequence

1. Promote reviewed changes through experimental → `dev` → `main`. The release
   version declarations and both installer fallback pins must match the stable
   version being released.
2. Require successful `main` push CI and certification at the source commit.
   Deploy the matching public installer through `deploy.yml` and verify the
   served bytes before starting the release.
3. Create and push an annotated `v<major>.<minor>.<patch>` tag at that reviewed
   `main` commit. Tag pushes do not start docs, binary releases, or public shell
   deployment. Publish docs with `gh workflow run pages.yml --ref main`.
4. Dispatch `release.yml` from `main` with its `tag` input set to that existing
   tag. For example, replace `vX.Y.Z` in
   `gh workflow run release.yml --ref main -f tag=vX.Y.Z`.
5. Wait for the entire release workflow, including public-install smoke and
   container packaging after publication. Release assets can already exist
   when a downstream check fails; their presence alone is not readiness.

Release metadata validates the resolved tag's version and installer data using
validators from `main`, checks the served installer pin, and checks exact-commit
CI/certification. Builds check out that resolved commit. The current validators
and installer require stable `X.Y.Z` versions, and publication sets
`prerelease: false`; prerelease tags are not a supported release path today.

The checksum manifest (`heiwa-<version>-checksums.txt`) verifies downloaded
archives. Unless `HEIWA_VERSION` is supplied, the installer resolves GitHub's
latest published release and falls back to its pinned version if lookup fails.
It does not install an unpublished tag merely because the tag exists.

### Release artifacts

For each successfully dispatched stable release, the workflow publishes:

```
heiwa-<version>-macos-aarch64.tar.gz
heiwa-<version>-linux-x86_64.tar.gz
heiwa-<version>-windows-x86_64.zip
heiwa-<version>-checksums.txt
heiwa-<version>-macos-aarch64-app.tar.gz
heiwa-<version>-macos-aarch64-app.tar.gz.sig
latest.json
```

Each CLI platform archive contains `heiwa` (or `heiwa.exe`), the compiled cockpit
under `cockpit/`, and the license/community files. The installer verifies the
archive checksum and rejects links or paths outside the versioned archive root
before extracting it.

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
|   Cloudflare DNS/Pages    GitHub Pages + Releases              |
|   (static shell/install)  (docs, source, binaries)              |
|                                                               |
+--------------------------------|------------------------------+
                                 | verified software installation
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

The docs workflow builds repository docs from protected `main` and publishes an Actions artifact to GitHub Pages. Cloudflare hosts the separately deployed static shell and installer. A docs deployment does not prove a binary release has published.

### Why JSONL plus Lance?

JSONL keeps durable truth inspectable, replayable, and Git-friendly. Lance gives
fast local vector recall without becoming a second authority; the index can be
rebuilt from the text corpus.

### Can I self-host the publishing pipeline?

A fork needs its own hosting configuration, domains, registry permissions, and release-signing setup. Update the Heiwa-specific repository and project references before publishing. Deploy the allowlisted public package to Cloudflare Pages, use the artifact-based docs workflow for GitHub Pages, and explicitly dispatch binary releases for reviewed annotated tags.

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
