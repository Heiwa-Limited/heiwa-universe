# HEIWA Context

Use this as a compact operator-facing summary, not as a replacement for the canonical repo truth.

## Read Order

1. `HEIWA.md`
2. `AGENTS.md`
3. `BUILD_MATRIX.md`
4. the relevant room file under `ops/rooms/`

## Architecture Of Record

- The installed `heiwa` runtime is the current product center.
- `heiwa-universe` is the canonical active workspace.
- Rust owns the primary runtime path in this repo.
- SpacetimeDB remains the authoritative state/evidence layer where the current runtime still depends on it.
- GitHub Actions and GitHub Pages are the active repo-native distribution surfaces.
- Legacy hosted paths may still exist in-tree, but they are not the default product story.

## Hard Rules

- Prefer local-first and installed-runtime framing over hosted-control-plane framing.
- Do not overstate maturity for web, hosted, or preview surfaces.
- Keep provider-owned behavior provider-owned.
- Prefer subscriptions/WebSockets over polling when live state matters.
- Cheapest acceptable route first.
- Sovereign work stays local-first.

## Work Routing

- For active implementation order and ownership, use `BUILD_MATRIX.md`.
- For platform/distribution work, load `ops/rooms/infra.md`.
- For runtime execution work, load `ops/rooms/execution.md`.
- For state and lease semantics, load `ops/rooms/control-plane.md`.
- For web/docs surface work, load `ops/rooms/web.md`.

## Transitional Boundary

- Some Python- and hosted-era material still exists as compatibility or historical reference.
- If docs conflict, `HEIWA.md` wins.
- If plans conflict, `BUILD_MATRIX.md` wins for current execution.
