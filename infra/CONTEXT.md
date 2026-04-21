# infra/ — Infrastructure Configs

Heiwa runs fully client-side as of 2026-04-21. No hosted backend. GitHub is the only publishing surface. Proper VPS comes post-traction.

## Split

| Directory | Scope |
| --- | --- |
| `local/` | **Local ops** — per-machine runtime (this client). Launchd/systemd units, node bootstrap, user-facing paths. |
| `platform/` | **Platform ops** — GitHub-hosted distribution. Actions, Pages, Releases, homebrew tap. Cloudflare DNS optional. |

## infra/local/

- `local/macos/` — macOS launchd plists, bootstrap
- `local/windows/` — WSL setup, Windows service scripts
- `local/systemd/` — Linux user units

Per-machine state: `~/.heiwa/state.db` (SQLite), OS keychain for OAuth tokens, `~/.heiwa/config.toml`.

## infra/platform/

- `platform/github/` — Actions workflows, Pages site config, Release metadata, homebrew tap
- `platform/cloudflare/` — optional DNS + edge terraform; activate only if a domain is purchased

## Node topology (client-side)

| Node | Hardware | Role |
| --- | --- | --- |
| MacBook M4 Pro (operator) | 24GB M4 | Primary operator seat, Ollama, Heiwa runtime |
| WSL / RTX 3060 (boost) | 32GB / 12GB VRAM | Optional GPU worker, embeddings, local media gen |

## Env vars

| Var | Purpose |
| --- | --- |
| `HEIWA_STATE_DIR` | Defaults to `~/.heiwa` |
| `HEIWA_NODE_ID` | Local node identifier |
| `HEIWA_CONFIG` | Path to `config.toml` |

## Deferred until VPS

- Heiwa identity service (today: GitHub device-flow OAuth)
- Cross-device state sync
- Marketplace backend
- STDB cloud plane — schema frozen as reference; do not invest further
