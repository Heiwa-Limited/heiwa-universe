# Infra Room

Load this room for:

- Cloudflare changes
- CI / deployment work
- runtime topology validation
- build and verification workflows

## Runtime Topology

- The MacBook checkout plus `~/.heiwa/` host current user functionality.
- Local JSONL is evidence authority; Lance is the derived recall plane.
- Cloudflare serves docs / marketing shell / install and update-manifest edge only after public access is re-enabled.
- Heiwa.app and the installed `heiwa` runtime run solely on user devices; Heiwa does not provide a hosted app/runtime service.
- WebSockets are the live transport for local cockpit status and future event subscriptions.

## Important Files

- `.github/workflows/deploy.yml`
- `apps/heiwa_core/`
- `apps/heiwa_orchestrator/`
- `crates/heiwa_evidence/` — JSONL journal truth under `~/.heiwa/evidence/`
- `crates/heiwa_embed/` — Lance derived recall index

(`crates/heiwa_stdb/` and `packages/heiwa_bindings/` were deleted in the
2026-07-15 backend pivot. They are not work targets.)

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
- Do not route durable control-plane state through external message brokers.
- Do not make Cloudflare or Discord look like the authority for runtime truth.
