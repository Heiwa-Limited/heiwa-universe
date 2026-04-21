# Heiwa

Heiwa is a local-first AI runtime and operator surface. The installed `heiwa` CLI is the current product center, and this repository is being prepared for GitHub-native distribution, documentation, and contributor intake.

## Supported surfaces

- Installed CLI runtime
- Rust execution kernel
- Provider discovery and routing
- Documentation published from this repository

Legacy hub, hosted, and experimental surfaces may still exist in the tree, but they should not be presented as equally mature public product surfaces.

## Current target architecture

- **Primary runtime**: local `heiwa` install
- **Execution stack**: Rust + TypeScript + Shell
- **Published docs**: GitHub Pages
- **Release channel**: GitHub Actions + Releases
- **Operator node**: MacBook M4 Pro 24GB

## Design intent

Heiwa is being hardened toward a cleaner, smaller public contract:

- keep the installed runtime as the product center
- prefer local execution and provider-owned runtimes over hosted abstractions
- make docs, CI, and releases coherent from a cold clone
- stop overstating legacy hosted/control-plane paths

## Truth boundary

If a surface is not covered by current docs, CI, or an explicit build matrix task, it should not be presented here as complete.

## Source of truth

- [`HEIWA.md`](https://github.com/Strategizing/heiwa-universe/blob/main/HEIWA.md)
- [`BUILD_MATRIX.md`](https://github.com/Strategizing/heiwa-universe/blob/main/BUILD_MATRIX.md)
- [`README.md`](https://github.com/Strategizing/heiwa-universe/blob/main/README.md)
