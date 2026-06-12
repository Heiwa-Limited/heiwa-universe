# Heiwa Runtime Baseline

Heiwa has two distinct upgrade surfaces:

1. **Repo/release baseline** for reproducible CI, release, docs, and optional remote-support builds.
2. **Operator baseline** for Devon's local machine and boost-node workflows.

The deploy baseline is pinned and conservative. The operator baseline can move faster, but only around the pinned repo contract.

## Deploy Baseline

These versions are the canonical floor for `heiwa-universe`:

| Surface | Baseline | Reason |
| --- | --- | --- |
| Rust | `1.93.1` | Required by `heiwa-core` and SpacetimeDB crates. |
| Docker rust-builder | `rust:1.93-slim` | Matches the workspace toolchain floor used in CI and optional remote-support builds. |
| Node | `24.14.1` | Stable LTS lane for TypeScript workspace and deploy tooling. |
| npm | bundled with Node 24 | Repo installs should follow the pinned Node lane. |
| Python | `3.14.x` | Current repo pytest/docs/runtime compatibility lane. |
| STDB auth | `spacetime login` shell identity targeting Maincloud | Canonical SpacetimeDB publisher/operator auth. No Heiwa-owned API key plane. |
| STDB token compat | `STDB_TOKEN` | Legacy/compat only for code paths that still require token-shaped material. |
| Machine auth | `HEIWA_MACHINE_AUTH_TOKEN` | Canonical worker and operator machine auth boundary. |
| Session auth | `HEIWA_JWT_SIGNING_SECRET` | Canonical user session signing boundary. |

### Non-negotiables

- Hosted deploys must build from the pinned Dockerfile and `/health` healthcheck.
- CI must set up Rust and Node explicitly instead of relying on runner defaults.
- Root TypeScript checks must run under the repo-pinned Node version.
- Local development may use newer global runtimes, but repo commands should use the pinned Rust and Node lanes.

## Operator Machine Baseline

Devon's machine is the operator and boost-node plane. It needs a wider tool surface than hosted services, but not every tool is production-critical.

### Required

- `git`
- `rustup`, `rustc`, `cargo`
- Node `24.x` available for repo work
- `python3` `3.14.x`
- `uv`
- `gh`

### Optional but expected

- `pnpm`
- `ollama`
- `tailscale`
- `wrangler` for Cloudflare/edge operations

### Recommended practice

- Keep a repo-compatible Node 24 toolchain available even if newer Node versions are installed globally.
- Use repo-local npm dependencies for TypeScript (`npm install` in the repo), not global `tsc`.
- Treat `brew upgrade` as a staged operator action, not a blanket cron job.
- Upgrade one package-manager lane at a time: Homebrew formulae, Rust toolchain, Node toolchain, Python/uv dependencies.

## Safe Update Order

1. Update repo pins and CI checks.
2. Verify local build and test commands against the pinned toolchains.
3. Upgrade operator-machine CLIs that are explicitly required by the repo.
4. Upgrade optional local tooling such as `ollama` and `tailscale` separately.

## Audit Commands

Run these from the repo root:

```bash
bash scripts/check_heiwa_core_dockerfile.sh
bash scripts/check_runtime_baseline.sh
bash scripts/audit_operator_machine.sh
```

## Local Runtime Notes

- `heiwa app start` is the canonical current user runtime server.
- Local `~/.heiwa` state must be enough for user functionality.
- STDB (`local` or `maincloud`) is sync/adjudication, not required for the local cockpit hot path.
