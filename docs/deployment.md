# Deployment

## Current publish path

The current platform goal is GitHub-native distribution:

- GitHub Actions validates the Rust workspace on macOS, Linux, and Windows.
- GitHub Pages publishes the docs site from `docs/` on release tags.
- GitHub Releases are the intended handoff point for packaged runtime artifacts.

This repo should be able to go from fresh clone to verified build and published docs without assuming Railway, Cloudflare, or a hosted control plane.

## CI contract

- `cargo build --workspace --locked`
- `cargo test --workspace --locked`
- `cargo clippy --workspace --locked --all-targets`
- `mkdocs build --strict`

## Docs publishing

The docs site is built by MkDocs Material and deployed by GitHub Pages from the generated `site/` directory. Publishing is tag-driven so the public docs track intentional release points instead of every `main` push.

## Legacy hosted paths

Hosted and control-plane material still exists in the repository as reference or migration context. It is not the primary release path for the current client-first build matrix, and it should not be described as the default operator experience.

## Verification

- CI must pass on all Rust matrix platforms before release work continues.
- Docs must build cleanly with `mkdocs build --strict`.
- Release automation should extend from this baseline rather than bypass it.
