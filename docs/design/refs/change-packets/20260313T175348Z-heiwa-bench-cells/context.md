# Figma Sync Context

- Profile: `heiwa-one-system`
- Repo: `Strategizing/heiwa-universe`
- Repo root: `/Users/dmcgregsauce/heiwa`
- Sync mode: `manual_packet`
- Generated (UTC): `2026-03-13T17:53:48Z`

## Intent

Update the Heiwa architecture visuals to reflect two new live seed surfaces:

- `HeiwaCells`: the current identity manifest is now materialized as a real cell catalog
- `HeiwaBench`: the route + cell selection release-gate now exists as a real benchmark runner

These surfaces are live through:

- CLI: `apps/heiwa_cli/heiwa`
- MCP/HTTP: `apps/heiwa_hub/mcp_server.py`
- CI: `.github/workflows/deploy.yml`

## Constraint Note

These are live seed implementations, not the final fully expanded product planes.

Show them as active system layers, but do not imply:

- full skill-pack installation UX
- red-team/fuzz coverage beyond the current release-gate suites
- complete proposal/lease/RFC migration
