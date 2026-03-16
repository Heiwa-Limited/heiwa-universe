# config/ — Configuration Layer

System-wide configuration for the Heiwa operating system.

## Subdirectories

| Directory | Purpose |
| --- | --- |
| `swarm/` | Runtime configs — ai_router, blueprints, end-state targets |
| `identities/` | Agent/cell identity definitions, persona, soul |
| `schemas/` | Data schemas |

## Key Files

| File | Purpose |
| --- | --- |
| `swarm/ai_router.json` | Model registry, provider routing, rate limits, compute classes |
| `swarm/BUILD_BLUEPRINT_2026-03-06.md` | Hardware topology, execution model, cost targets |
| `swarm/END_STATE_2026-03.md` | Target architecture, kill list, what's done vs pending |
| `identities/profiles.json` | HeiwaCells agent catalog |
| `identities/persona/identity.md` | Persona template |
| `identities/soul/core.md` | Continuity/persona layer |
