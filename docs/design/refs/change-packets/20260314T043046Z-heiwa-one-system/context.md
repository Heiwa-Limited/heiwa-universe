# Heiwa Figma Sync Packet

- Generated at: `2026-03-14T04:30:46Z`
- Profile: `heiwa-one-system`
- Repo root: `/Users/dmcgregsauce/heiwa`
- Scope: OAuth routing topology, Railway-first control plane, CLI entrypoint cleanup

## Why This Packet Exists

The repo topology and runtime routing changed in ways that affect the architecture diagram:

- OAuth-backed model lanes are now first-class routing providers instead of a generic premium bucket.
- Railway remains the always-on control plane.
- MacBook and WSL stay modular and ephemeral.
- SpacetimeDB remains the durable state layer.
- NATS remains ephemeral transport only.

## Canonical Runtime Decisions

- Railway plan assumption for current diagrams: `32 GB RAM`, `32 vCPU`, `100 GB shared disk`, billed as `$20 USD/month` and observed by operator as about `$25 CAD/month`.
- Persistent cloud services: `heiwa-cloud-hq`, `nats`, `spacetimedb`, `heiwa-scheduler`.
- Ephemeral execution nodes: `macbook@heiwa-node-a`, `pc@heiwa-node-b`.
- Routing posture: cheapest acceptable route first, privacy-first, local-first before remote, subscription OAuth lanes before paid API overflow.

## Routing Changes To Reflect

- `google-gemini-cli` is a direct Gemini CLI OAuth lane for research and long-context work.
- `google-antigravity` is a distinct Google OAuth lane routed through an isolated OpenClaw profile for strategic work.
- `claude-code` is a direct Claude Code OAuth lane for review/adversarial work.
- `codex` is a direct Codex CLI OAuth lane for build-heavy implementation.
- Local `ollama` and `local` providers remain the privacy-preserving fast path.

## Surface Changes To Reflect

- Bare `heiwa` no longer auto-enters the legacy TUI path. It now presents the CLI surface and natural-language dispatch hint.
- The provider registry in `config/swarm/ai_router.json` is the canonical routing inventory.
- `config/identities/profiles.json` now uses explicit OAuth-backed model ids for research, strategy, review, and build cells.
