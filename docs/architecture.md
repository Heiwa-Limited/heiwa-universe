# Architecture

## Product split

Heiwa uses a narrow split between execution, state, and presentation:

- **Installed runtime:** `heiwa` on the local machine is the primary operator surface.
- **Rust workspace:** runtime execution, provider supervision, session flow, and CLI/TUI behavior live in `apps/` and `crates/`.
- **SpacetimeDB:** remains the authoritative state/evidence plane where the current runtime still depends on it.
- **Docs and release surfaces:** GitHub Actions and GitHub Pages are the current repo-native distribution surfaces.

## Surface boundaries

- The installed runtime is the current product center.
- Docs are explanatory and release-facing, not authoritative runtime surfaces.
- Web or hosted shells may exist, but they should not be described as more mature than the installed runtime.
- Presentation layers must not duplicate privileged runtime behavior or pretend to own execution truth.

## Repo boundaries

- `heiwa-universe` is the canonical repo for the current stack.
- Legacy or compatibility material may remain in-tree, but `HEIWA.md` and the current build matrix define the active contract.

## State bindings

- `apps/heiwa_hub/spacetimedb/` is the Rust SpacetimeDB module.
- `packages/heiwa_bindings/rust/` and `packages/heiwa_bindings/typescript/` are generated from that module.
- Python currently uses the typed bridge in `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` until a stable generator path is adopted.
