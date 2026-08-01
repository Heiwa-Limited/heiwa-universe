# Heiwa Runtime Baseline

Heiwa has two distinct upgrade surfaces:

1. **Repo/release baseline** for reproducible CI, release, docs, and optional remote-support builds.
2. **Operator baseline** for Devon's local machine and boost-node workflows.

The deploy baseline is pinned and conservative. The operator baseline can move faster, but only around the pinned repo contract.

## Deploy Baseline

These versions are the canonical floor for `heiwa-universe`:

| Surface             | Baseline                   | Reason                                                                               |
| ------------------- | -------------------------- | ------------------------------------------------------------------------------------ |
| Rust                | `1.95.0`                   | Pinned by `rust-toolchain.toml`, CI, and the Rust builder image.                      |
| Docker rust-builder | `rust:1.95-slim`           | Matches the workspace toolchain used in CI and optional remote-support builds.        |
| Node                | `26.0.0`                   | Pinned by `.nvmrc` and `.node-version` for TypeScript and deploy tooling.             |
| npm                 | bundled with Node 26       | Repo installs follow the pinned Node lane and root npm workspaces.                    |
| Python              | `3.14.x`                   | Current repo pytest/docs/runtime compatibility lane.                                 |
| Machine auth        | `HEIWA_MACHINE_AUTH_TOKEN` | Canonical worker and operator machine auth boundary.                                 |
| Session auth        | `HEIWA_JWT_SIGNING_SECRET` | Canonical user session signing boundary.                                             |

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
- Node `26.x` active for repo work
- `python3` `3.14.x`
- `uv`
- `gh`

### Optional but expected

- `pnpm`
- `ollama`
- `tailscale`
- `wrangler` for Cloudflare/edge operations

### Recommended practice

- Keep Node 26 active for repo work; provider-owned runtimes such as Hermes may keep private Node versions under their own state roots.
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
- JSONL under `~/.heiwa/evidence/` is canonical evidence truth; Lance is a derived local index.
- GitHub evidence sync is planned and redaction-gated; it is not a current runtime dependency.
