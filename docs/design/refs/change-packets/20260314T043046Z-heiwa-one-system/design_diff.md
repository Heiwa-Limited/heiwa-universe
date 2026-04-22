# Design Diff

## Update

- Replace any generic "premium remote models" block with four explicit routing lanes: `Gemini CLI`, `Antigravity`, `Claude Code`, and `Codex`.
- Show `Antigravity` as a distinct Google OAuth rate group, routed through an isolated OpenClaw profile, not as the same lane as Gemini CLI.
- Show `heiwa-cloud-hq` on Railway as the always-on control plane that owns routing, automation, and session coordination.
- Keep `SpacetimeDB` visually paired with the control plane as durable state.
- Keep `NATS` visually paired with the control plane as transient dispatch only.
- Show `MacBook Node A` and `WSL/PC Node B` as ephemeral executors, not permanent infra.

## Add

- Railway plan annotation: `$20 USD/month`, `32 GB RAM`, `32 vCPU`, `100 GB shared disk`.
- Provider metadata callouts for `transport`, `auth kind`, and `rate group`.
- A short note that the provider registry in `ai_router.json` is the routing source of truth.

## Remove

- Any visual that implies a localhost-first NATS default for the main operator path.
- Any visual that implies the bare `heiwa` command launches directly into the old sovereign shell by default.
- Any merged Gemini/Antigravity lane or direct dependency on local always-on daemons.
