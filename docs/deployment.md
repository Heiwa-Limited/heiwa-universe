# Deployment

## Current publish path

The current platform goal is GitHub-native distribution:

- GitHub Actions validates the Rust workspace on macOS, Linux, and Windows.
- GitHub Pages publishes the docs site from `docs/` on release tags.
- GitHub Releases publish tagged `heiwa` archives plus checksums through `.github/workflows/release.yml`.
- Release archives include `README.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, and `LICENSE`.

This repo should be able to go from fresh clone to verified build and published docs without assuming Cloudflare or a hosted control plane.

## Source Promotion Contract

Under the local-first promotion posture, local sandbox verification is the primary gate before updating `main` directly on the local machine. GitHub Actions are minimal, only used as a back-end emergency or release archiver, and must not block local development progress.

The local-first promotion path is:

1. Work happens on a branch.
2. Run the local development sandbox checks to verify the build, tests, and documentation.
3. Once all local sandbox checks pass, merge the branch directly into `main` locally.
4. The local installed runtime promotes from the local checkout source using `heiwa app update --source checkout`.
5. GitHub serves primarily as a remote backup, tag archiver, and distribution mechanism rather than a build/test gatekeeper. Commits can be pushed to remote `main` only after passing the local sandbox gate.

The MacBook has owner permissions to bypass remote CI blockers, shifting verification authority entirely to local sandbox runs. The MacBook's runtime updates through local checkout source promotion (`--source checkout`) rather than waiting for remote build artifacts.

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

Use the local sandbox as the primary verification gate before merging to `main` locally or performing any release updates.

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
- GitHub Releases serve as a secondary emergency archive; local sandbox verification remains the canonical gate.

## Protected Backend Workflow

GitHub and Cloudflare are protected company services with separate authority.

| Backend    | Protected role                                               | Safe update path                                                       |
| ---------- | ------------------------------------------------------------ | ---------------------------------------------------------------------- |
| GitHub     | source, tag archive, releases, checksums, install provenance | local sandbox checks passed, local merge, push to main, tagged release |
| Cloudflare | DNS, WAF, public docs/app shell, install/update front door   | GitHub-driven deploys or explicit owner-approved `wrangler` deploys    |

Rules:

- Do not use Cloudflare as a second binary/source authority. It may front
  GitHub-backed install and update metadata.
- Do not put provider secrets, Cloudflare tokens, or user runtime
  state in GitHub.
- Keep user-machine state under `~/.heiwa`; any future GitHub evidence projection must be redacted and explicitly enabled.
- Bypassing GitHub Actions and CI checks is standard posture; verify entirely in local sandbox instead of waiting for remote checks.

## CI contract

- `cargo build --workspace --locked`
- `cargo test --workspace --locked`
- `cargo clippy --workspace --locked --all-targets`
- `uv run --extra dev python -m pytest`
- `mkdocs build --strict`

The default Python gate intentionally excludes legacy Hub tests. Run `uv run --extra dev python -m pytest legacy/apps/heiwa_hub/tests` when repairing or promoting that surface (the hub was quarantined under `legacy/`).

## Docs publishing

The docs site is built by MkDocs Material and deployed by GitHub Pages from the generated `site/` directory. Publishing is tag-driven so the public docs track intentional release points instead of every `main` push.

## Legacy hosted paths

Hosted and control-plane material still exists in the repository as reference or migration context. It is not the primary release path for the current client-first build matrix, and it should not be described as the default operator experience.

## Verification

- CI must pass on all Rust matrix platforms before release work continues.
- Docs must build cleanly with `mkdocs build --strict`.
- Release automation should extend from this baseline rather than bypass it.
- Release asset names and checksum output should stay aligned with `infra/platform/github/README.md`.
- License and package metadata must pass `scripts/check_release_metadata.sh` before release work continues.
