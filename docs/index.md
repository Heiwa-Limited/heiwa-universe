# Heiwa

Heiwa is a local-first AI runtime and operator surface. The installed `heiwa`
runtime and HOME-local `Heiwa.app` are the current product center, and this
repository is being prepared for GitHub-native distribution, documentation, and
contributor intake.

The first customer is Devon. The first product is the personal operator system that safely joins local models, provider CLIs, computer use, messaging surfaces, files, and personal workflows through one governed runtime.

## What Heiwa does

> Heiwa watches what matters, summarizes what changed, stages what needs action, executes what is safe, and proves what happened.

Three planes compose one flow:

- **Intake** — operator command bar and passive feeds (mail, calendar, messages, files, runtime alerts).
- **Execution** — DREX routes work to local models, provider CLIs, tools, and connectors under leases and approval gates.
- **Evidence** — every read or action emits a source-linked receipt, mirrored to SpacetimeDB when online.

Maturity is uneven across planes today; see [`HEIWA.md`](https://github.com/Strategizing/heiwa-universe/blob/main/HEIWA.md#the-three-planes) for current vs target.

## Supported surfaces

- Installed `heiwa` runtime
- HOME-local Heiwa.app primary input/display surface over the same runtime
- Rust execution kernel
- Provider discovery and routing
- Account/tool/model capability fabric
- SpacetimeDB-backed evidence and state direction
- Documentation published from this repository

Legacy hub, hosted, and experimental surfaces may still exist in the tree, but they should not be presented as equally mature public product surfaces.

## Current target architecture

- **Primary runtime**: local `heiwa` install
- **Execution stack**: Rust + TypeScript + Shell
- **Published docs**: GitHub Pages
- **Release channel**: GitHub Actions + Releases
- **Hosted backbone target**: GitHub + Cloudflare + SpacetimeDB
- **Operator node**: MacBook M4 Pro 24GB

## Design intent

Heiwa is being hardened toward a cleaner, smaller public contract:

- keep the installed runtime as the product center
- prefer local execution and provider-owned runtimes over hosted abstractions
- use hosted infrastructure for public clients, releases, state, subscriptions, and evidence
- turn external accounts and tools into modular capability lanes with explicit scopes and leases
- make docs, CI, and releases coherent from a cold clone
- stop overstating legacy hosted/control-plane paths

## Truth boundary

If a surface is not covered by current docs, CI, or an explicit build matrix task, it should not be presented here as complete.

## Source of truth

- [`HEIWA.md`](https://github.com/Strategizing/heiwa-universe/blob/main/HEIWA.md)
- [`docs/product-contract.md`](product-contract.md)
- [`docs/capability-fabric.md`](capability-fabric.md)
- [`BUILD_MATRIX.md`](https://github.com/Strategizing/heiwa-universe/blob/main/BUILD_MATRIX.md)
- [`README.md`](https://github.com/Strategizing/heiwa-universe/blob/main/README.md)
