# Web Room

Load this room for:

- docs-site and public-web contract changes
- app-shell naming and routing cleanup
- browser-facing auth flow changes
- static vs generated site boundary decisions

## Current Truth

- The installed `heiwa` runtime is the product center.
- Web surfaces are attached presentation layers, not the authority for runtime truth.
- GitHub Pages is the current canonical docs publishing path for this repo.
- Browser shells should consume stable APIs and runtime state honestly, without implying that the web layer is the product center.

## Important Files

- `apps/heiwa_web/`
- `docs/`
- `mkdocs.yml`
- `.github/workflows/pages.yml`

## Web Rules

- Do not make the web shell look like a second control plane.
- Do not claim public domains or hosted deploy paths are primary unless they are actually shipped and verified.
- Keep marketing/docs/status surfaces clearly separated from privileged operator/runtime behavior.
- Prefer simplifying or removing stale public host-map language over preserving elaborate future-state drafts.
