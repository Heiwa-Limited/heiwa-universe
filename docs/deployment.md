# Deployment

## Current publish path

The current platform goal is GitHub-native distribution:

- GitHub Actions validates the Rust workspace on macOS, Linux, and Windows.
- GitHub Pages publishes the docs site from `docs/` on release tags.
- GitHub Releases publish tagged `heiwa` archives plus checksums through `.github/workflows/release.yml`.
- Release archives include `README.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, and `LICENSE`.

This repo should be able to go from fresh clone to verified build and published docs without assuming Cloudflare or a hosted control plane.

## Source Promotion Contract

GitHub is the company source of truth. A developer machine, including Devon's
MacBook, is a client of that source, not the authority.

The normal promotion path is:

1. Work happens on a branch.
2. The branch is pushed to GitHub.
3. GitHub PR review and checks decide whether the change is promotable.
4. `main` moves only through reviewed commits or explicitly approved owner
   actions.
5. Releases, checksums, installers, and docs are produced from GitHub state.
6. User machines update or install from the GitHub-backed release/install path.
7. Local runtimes report their installed version, source tag/commit, machine id,
   and update/restart state.

The MacBook may have owner permissions, but it should still exercise the same
install/update path a normal user machine would use. Owner permission changes
what Devon is allowed to approve; it should not change the shape of the runtime.

## Development vs Installed Runtime

There are two legitimate local modes:

| Mode | Source | Purpose | Port guidance |
| --- | --- | --- | --- |
| Installed user mode | GitHub-backed installed `heiwa` binary under `~/.heiwa` | Real operator use | `7474` |
| Checkout development mode | local branch in this checkout | Verify code before GitHub promotion | temporary alternate port such as `7475` |

Do not blur these modes. If a checkout runtime works on `7475`, that proves the
branch can serve the endpoint. It does not prove the installed product runtime on
`7474` has been updated. The installed runtime becomes current only after the
source is promoted and the machine updates through the install/update path.

## Protected Backend Workflow

GitHub, Cloudflare, and SpacetimeDB are protected company backends with separate
authority.

| Backend | Protected role | Safe update path |
| --- | --- | --- |
| GitHub | source, CI, releases, checksums, install provenance | branch, PR, checks, merge, tagged release |
| Cloudflare | DNS, WAF, public docs/app shell, install/update front door | GitHub-driven deploys or explicit owner-approved `wrangler` deploys |
| SpacetimeDB | canonical state, reducers, leases, evidence, subscriptions | schema/reducer changes reviewed in repo, bindings regenerated, publish gated separately |

Rules:

- Do not use Cloudflare as a second binary/source authority. It may front
  GitHub-backed install and update metadata.
- Do not mutate SpacetimeDB production schema or reducers directly from an agent
  session unless the operator explicitly approves a production publish.
- Do not put provider secrets, Cloudflare tokens, STDB tokens, or user runtime
  state in GitHub.
- Keep user-machine state under `~/.heiwa` and sync only approved evidence or
  reducer-backed state to SpacetimeDB.
- Treat failed GitHub checks as a stop condition for merge unless the owner
  explicitly chooses to bypass protection.

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
