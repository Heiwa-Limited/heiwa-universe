# Infra Room

Load this room for:

- GitHub Actions and release automation
- docs publishing and repo-distribution work
- runtime topology validation
- build and verification workflows

## Runtime Topology

- The installed `heiwa` runtime is the primary operator surface.
- GitHub is the primary distribution and automation surface for this repo.
- SpacetimeDB remains a backend/state authority where the current runtime still depends on it.
- Hosted infra may exist, but it is not the default product story for current repo/platform work.

## Important Files

- `.github/workflows/ci.yml`
- `.github/workflows/pages.yml`
- `.github/workflows/deploy.yml`
- `mkdocs.yml`
- `apps/heiwa_hub/main.py`
- `apps/heiwa_hub/mcp_server.py`
- `apps/heiwa_hub/scripts/generate_spacetimedb_bindings.sh`
- `packages/heiwa_bindings/*`

## Current CI Expectations

- hub smoke tests
- agent context map test
- HeiwaCells catalog test
- HeiwaBench release-gate test
- docs build
- static web checks
- repo hygiene

## Infra Rules

- Do not add polling-oriented public surfaces where subscriptions or WebSockets belong.
- Do not route durable state through external message brokers.
- Do not make docs, status shells, or hosted edges look like the authority for runtime truth.
- Do not overstate legacy hosted paths when the task is really about repo distribution or local runtime behavior.
