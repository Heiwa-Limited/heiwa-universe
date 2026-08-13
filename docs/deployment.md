# Deployment

## Current publish path

The current platform goal is GitHub-native distribution:

- GitHub Actions validates the Rust workspace on macOS, Linux, and Windows.
- GitHub Pages publishes the docs site from `docs/` on release tags.
- GitHub Releases publish tagged `heiwa` archives plus checksums through `.github/workflows/release.yml`.
- Release archives include `README.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, and `LICENSE`.

This repo should be able to go from fresh clone to verified build and published docs without assuming Cloudflare or a hosted control plane.

## Source Promotion Contract

GitHub is the canonical source, promotion, and release authority. Local sandbox
verification supplies independent evidence before a push; protected GitHub
checks decide whether a pull request may update `main`.

The promotion path is:

1. Work happens on a branch.
2. Run the local development sandbox checks for build, tests, documentation,
   security, packaging, and checkout-runtime behavior.
3. Push the branch and open or update a pull request targeting `main`.
4. Require every protected GitHub check to pass on the current pull-request
   head and merge result before merging.
5. Merge through the protected pull request; do not push directly to `main`.
6. Tag the verified `main` commit and let the release workflow publish archives,
   checksums, attestations, and the container image.
7. Update installed runtimes from the verified GitHub Release and record the
   before/after receipt.

`heiwa app update --source checkout` remains a developer and recovery path. It
must not be presented as a public release or used to bypass a failed protected
check. Any emergency use requires the same local gates and an explicit receipt
identifying the checkout commit.

## Development vs Installed Runtime

There are two legitimate local modes:

| Mode                      | Source                                                  | Purpose                             | Port guidance                           |
| ------------------------- | ------------------------------------------------------- | ----------------------------------- | --------------------------------------- |
| Installed user mode       | GitHub-backed installed `heiwa` binary under `~/.heiwa` | Real operator use                   | `7474`                                  |
| Checkout development mode | local branch in this checkout                           | Verify code before GitHub promotion | temporary alternate port such as `7475` |

Do not blur these modes. If a checkout runtime works on `7475`, that proves the
branch can serve the endpoint. It does not prove the installed product runtime on
`7474` has been updated. The installed runtime becomes current only after the
source is promoted and the machine updates through the install/update path.

CLI contract:

- `heiwa app update --dry-run` describes the GitHub Releases update path.
- `heiwa app update --source checkout --dry-run` describes developer reinstall
  from the current checkout.

## Local Development Sandbox

Use the local sandbox as an independent preflight before pushing a promotion
candidate or performing release work.

Build/test gate:

```bash
cargo test -p heiwa-shell
uv run --extra docs mkdocs build --strict
bash scripts/check_release_metadata.sh
```

Release gate:

```bash
bash scripts/package_release_sandbox.sh --version dev-local
```

The sandbox script builds the host-platform release binary, packages the same
runtime files as the GitHub release workflow, writes a checksum manifest, and
smokes the packaged `heiwa` binary with `app update --dry-run`. It writes under
`/tmp/heiwa-release-sandbox` by default and does not install, upload, tag, or
mutate `~/.heiwa`.

Deployment gate:

- Cloudflare changes must be tested against local/static build output first.
- Evidence schema and Lance migration changes must be tested locally before promotion.
- GitHub Releases are the canonical public binary authority; local sandbox
  output is evidence, not a substitute release channel.

## Protected Backend Workflow

GitHub and Cloudflare are protected company services with separate authority.

| Backend    | Protected role                                               | Safe update path                                                       |
| ---------- | ------------------------------------------------------------ | ---------------------------------------------------------------------- |
| GitHub     | source, protected promotion, releases, checksums, install provenance | local preflight, protected pull request, tagged release |
| Cloudflare | DNS, WAF, public docs/app shell, install/update front door   | GitHub-driven deploys or explicit owner-approved `wrangler` deploys    |

Rules:

- Do not use Cloudflare as a second binary/source authority. It may front
  GitHub-backed install and update metadata.
- Do not put provider secrets, Cloudflare tokens, or user runtime
  state in GitHub.
- Keep user-machine state under `~/.heiwa`; any future GitHub evidence projection must be redacted and explicitly enabled.
- Do not bypass required GitHub checks. A local result and a hosted result prove
  different environments; both are required for a public release candidate.

## CI contract

Pull requests are the sub-minute feedback gate:

- Linux unit/integration tests via three drift-checked `cargo nextest` package groups
- Rust formatting, Clippy, and unused-dependency checks
- dependency diff review, Gitleaks, and Trivy
- TypeScript/web lint, docs, agent sync, and repository contracts

Protected `main` is the full release certification gate:

- the standard Cargo test harness on Linux
- native macOS and Windows test-target compilation
- Tauri desktop-shell compilation on macOS
- Lance backend and journal-rebuild integration tests
- full Rust, npm, Python, and Deno security audits

`release.yml` queries the Actions API and requires that full `main` workflow to
have succeeded at the exact annotated-tag commit before any release build starts.

The default Python gate intentionally excludes legacy Hub tests. Run `uv run --extra dev python -m pytest legacy/apps/heiwa_hub/tests` when repairing or promoting that surface (the hub was quarantined under `legacy/`).

## Docs publishing

The docs site is built by MkDocs Material and deployed by GitHub Pages from the generated `site/` directory. Publishing is tag-driven so the public docs track intentional release points instead of every `main` push.

## Legacy hosted paths

Hosted and control-plane material still exists in the repository as reference or migration context. It is not the primary release path for the current client-first build matrix, and it should not be described as the default operator experience.

## Verification

- The fast PR gate must pass before merge; the full `main` certification must
  pass at the exact tag commit before release work continues.
- Docs must build cleanly with `mkdocs build --strict`.
- Release automation should extend from this baseline rather than bypass it.
- Release asset names and checksum output should stay aligned with `infra/platform/github/README.md`.
- License and package metadata must pass `scripts/check_release_metadata.sh` before release work continues.
