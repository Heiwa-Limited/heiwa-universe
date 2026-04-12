# Runtime-Owned Heiwa

Heiwa owns the harness. Installed runtime authority lives under `~/.heiwa/`.

## Authority Order

1. `~/.heiwa/`
2. explicit project overlays Heiwa reads
3. generated provider projections under `~/.heiwa/generated/`
4. provider-owned auth and native runtime state

## Ownership

- Providers own auth and inference internals.
- Heiwa owns harness sessions, sandboxes, routing policy, runtime state, and evidence UX.
- SpacetimeDB remains backend authority, not normal user-facing control surface.

## Runtime Layout

```text
~/.heiwa/
  bin/
  providers/
  models/
  capabilities/
  modes/
  policies/
  generated/
  sessions/
  artifacts/
  logs/
  cache/
  state/
  secrets/
```

Important paths:

- `~/.heiwa/providers/registry.json`
- `~/.heiwa/providers/legacy_connections.json`
- `~/.heiwa/state/identity.json`
- `~/.heiwa/state/connection.json`
- `~/.heiwa/modes/concise/MODE.md`

## Migration

Current runtime bootstrap migrates forward from legacy flat-root files if present:

- `~/.heiwa/accounts.json`
- `~/.heiwa/provider_connections.json`
- `~/.heiwa/identity.json`
- `~/.heiwa/connection.json`

Phase 1 is safe migrate-forward only. Old files are not deleted yet.

## Repo Overlays

Repo-local `.codex/`, `.claude/`, and `.gemini/` files are project overlays. They are not canonical user authority. Keep them minimal and project-scoped.

## Installer Truth

Canonical hosted installer payload lives at `apps/heiwa_cli/scripts/install_heiwa.sh`.

Current OSS-first truth:

- source install works now
- runtime root bootstrap works now
- generated provider projections exist under `~/.heiwa/generated/`
- provider-home sync remains later work unless explicitly verified
