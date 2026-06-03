# Publishing Pipeline

How the `heiwa-universe` repository becomes the public Heiwa surface. Cloudflare delivers the shop window. GitHub stays authoritative for source, releases, and docs. SpacetimeDB adjudicates receipts when the runtime is online.

> Heiwa.ltd delivers software. The operator machine runs the runtime.

## Three publishing planes

| Plane | Surface | Source in repo | Authority |
| --- | --- | --- | --- |
| **Marketing shell** | `heiwa.ltd` | `apps/heiwa_app/clients/web/` | Cloudflare Pages |
| **Documentation** | `docs.heiwa.ltd` | `docs/` + `mkdocs.yml` | GitHub Pages |
| **Releases** | GitHub Releases | `apps/heiwa_core/`, `apps/heiwa_shell/` | GitHub Releases |
| **Evidence + licence state** | (internal) | `apps/heiwa_orchestrator/src/stdb/`, `crates/heiwa_stdb/` | SpacetimeDB |

Each plane has a single source of truth in the repo and a single deploy path. Automated workflows are the normal channel; a [manual fallback](#manual-fallback-when-actions-are-paused) exists for the periods when GitHub Actions are paused.

## Cloudflare — the static shop window

`heiwa.ltd` is a static surface. It exists to deliver the installer, marketing copy, install funnel, support routing, and the identity-exchange touchpoint. **It does not execute operator work.**

- **Cloudflare Pages project**: `heiwa-clients`
- **Build output**: `apps/heiwa_app/clients/web/` (static HTML + CSS + JS)
- **Wrangler config**: [`apps/heiwa_app/wrangler.toml`](https://github.com/Strategizing/heiwa-universe/blob/main/apps/heiwa_app/wrangler.toml)
- **Terraform**: [`infra/platform/cloudflare/main.tf`](https://github.com/Strategizing/heiwa-universe/blob/main/infra/platform/cloudflare/main.tf) — DNS, project, custom domain
- **Routes**: `heiwa.ltd` -> marketing/install/support; `docs.heiwa.ltd` -> documentation; `status.heiwa.ltd` -> read-only WebSocket health surface. The primary app is HOME-installed at `~/.heiwa/app/Heiwa.app`, not hosted at `app.heiwa.ltd`.

### What Cloudflare must never receive

- Operator state, memory, sessions, or evidence
- Provider secrets, API keys, OAuth tokens
- SpacetimeDB credentials or reducer authority

If a future feature appears to need any of the above on Cloudflare, treat it as a design escape and route through governance before shipping.

## GitHub — the authoritative repository

GitHub is the source of truth. Every public artifact is built from a tagged commit on `main`.

| Workflow | Trigger | Output |
| --- | --- | --- |
| [`ci.yml`](https://github.com/Strategizing/heiwa-universe/blob/main/.github/workflows/ci.yml) | every push + PR | `cargo build --workspace`, smoke tests, lint |
| [`pages.yml`](https://github.com/Strategizing/heiwa-universe/blob/main/.github/workflows/pages.yml) | tag push `v*` | MkDocs build → GitHub Pages → `docs.heiwa.ltd` |
| [`release.yml`](https://github.com/Strategizing/heiwa-universe/blob/main/.github/workflows/release.yml) | tag push `v*` | Cross-platform binaries → GitHub Releases |
| [`deploy.yml`](https://github.com/Strategizing/heiwa-universe/blob/main/.github/workflows/deploy.yml) | manual / push to `main` | Cloudflare Pages publish for `clients/web/` |

### Current pipeline status (2026-05-25)

The four workflows above are **runner-billed-out, not code-broken**. The GitHub account's spending-limit annotation blocks the runners; a `v*` tag push queues but does not execute. The code paths in `.github/workflows/*.yml` remain valid — restoring runner budget re-enables them with no code change required.

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

The result is byte-identical to what `release.yml` produces. The checksums manifest stays authoritative.

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

## SpacetimeDB — the evidence plane

When the installed runtime is online, Heiwa mirrors a narrow slice of state to SpacetimeDB as the **backend authority** for receipts, leases, and licence facts.

- **Adjudication crate**: [`crates/heiwa_stdb/`](https://github.com/Strategizing/heiwa-universe/tree/main/crates/heiwa_stdb)
- **Reducers**: WASM-compiled, called over WebSocket. Sub-millisecond internal latency. **Not** a REST/HTTP API.
- **Orchestrator binding**: [`apps/heiwa_orchestrator/src/stdb/`](https://github.com/Strategizing/heiwa-universe/tree/main/apps/heiwa_orchestrator/src/stdb)

### What STDB stores

- Receipt headers — never operator memory, prompts, or model outputs
- Licence keys and entitlement state
- Lease metadata for cross-device sessions
- Audit-trail breadcrumbs that link a receipt to the runtime that produced it

### What STDB does **not** store

- Operator memory, conversations, or session content
- Provider secrets
- Local model weights or inference outputs

This boundary is enforced in code in `crates/heiwa_stdb`. If a reducer signature appears to need richer payloads, route the design through governance — the public/runtime boundary is a non-negotiable.

### Maturity statement

STDB integration is wired and active for receipt mirroring and licence state. Lease coordination across multiple operator devices is partial today; see [`HEIWA.md`](https://github.com/Strategizing/heiwa-universe/blob/main/HEIWA.md) for the current vs target capability matrix.

## Operator boundary diagram

```
+---------------------------------------------------------------+
|                       PUBLIC BACKBONE                         |
|                                                               |
|   Cloudflare Pages        GitHub                STDB Cloud    |
|   (heiwa.ltd)             (source, releases,    (receipts,    |
|   static only             docs.heiwa.ltd)       licence)      |
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
|   nothing here leaves except a narrow receipt header to STDB  |
+---------------------------------------------------------------+
```

## Common operator questions

### Does Heiwa run on Cloudflare?

No. `heiwa.ltd` is a static site delivered by Cloudflare Pages. The runtime, app, memory, and provider secrets all live on the operator machine.

### Why GitHub Pages for docs and not Cloudflare?

Docs are tightly coupled to source — every tag publishes both. GitHub Pages keeps the doc surface authoritative against the commit it was built from. Cloudflare hosts the marketing surface where doc-source coupling is not a requirement.

### Why SpacetimeDB for evidence and not a plain database?

WASM reducers give us sub-millisecond, schema-validated state transitions with a WebSocket transport that does not require operator infrastructure. Heiwa never needs to run a server-side database — STDB is the backend authority operators can read from but never administer.

### Can I self-host the publishing pipeline?

The repository is the entire surface. Fork it, point a Pages project at `clients/web/`, and run MkDocs against `docs/` to get an isolated mirror. The release workflow is tagged-trigger driven and runs in any GitHub Actions account with no Heiwa-specific secrets beyond release signing.

## Change-control rules

- Cloudflare Pages config changes go through `infra/platform/cloudflare/main.tf` — never through the dashboard.
- New workflows or workflow edits land in `.github/workflows/` with the same review gate as runtime code.
- STDB reducer signatures are stamped against a published contract in `crates/heiwa_stdb/`; breaking changes require a major-version tag.
- The public/runtime boundary is a doctrine line. If a publishing change appears to need operator state on a public surface, route through [governance](support.html#governance--boundaries) before opening the PR.

## Where to next

- [Install Guide](https://heiwa.ltd/download.html) — operator-facing install path
- [Architecture](architecture.md) — full runtime architecture
- [Security](security.md) — disclosure policy and runtime threat model
- [Operator Runbook](operator-runbook.md) — day-to-day operation
