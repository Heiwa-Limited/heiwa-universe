# Heiwa Runtime Executive Alignment Sync Packet

- Generated at: `2026-03-15T18:11:42Z`
- Profile: `heiwa-one-system`
- Sync mode: `manual_packet`
- Repo root: `/Users/dmcgregsauce/heiwa`

## Why This Packet Exists

The local execution substrate drifted away from the Heiwa routing contract.

- `ai_router.json` already described the Class 3 lanes as `executive_full_access`.
- The live wrappers and launcher still contained narrower runtime defaults:
  - Gemini wrapper was previously hard-clamped to `plan`.
  - Claude wrapper was previously hard-clamped to `plan`.
  - Codex wrapper still defaulted to `workspace-write`.
  - Multiple wrappers fell back to `apps/heiwa_cli` or `apps/` instead of the monorepo root.
- Local tool configs also drifted:
  - OpenClaw defaulted to a non-local-first model mix and the wrong workspace.
  - PicoClaw still pointed at an obsolete Heiwa identity map path.

## Runtime Truth After This Pass

- The Heiwa launcher always exports `HEIWA_ROOT` and `HEIWA_WORKSPACE_ROOT`.
- `CLIContext` also exports those roots so subprocesses inherit the canonical monorepo path even when the CLI is already inside the venv.
- Gemini, Claude, Codex, OpenClaw, OpenCode, Antigravity, PicoClaw, and Ollama wrappers now resolve the Heiwa monorepo root consistently.
- Heiwa execution defaults now match the executive routing contract:
  - Gemini: `yolo`
  - Claude: `bypassPermissions`
  - Codex: `danger-full-access`
- OpenClaw and PicoClaw are aligned to Heiwa’s local-first posture instead of stale remote-first defaults.

## Verification Snapshot

- Gemini wrapper: direct wrapper invocation, no manual env injection, file-read probe passed.
- Claude wrapper: direct wrapper invocation, no manual env injection, file-read probe passed.
- Codex wrapper: direct wrapper invocation, no manual env injection, file-read probe passed.
- `gate_build.py`: passes after root-discovery and stale-path corrections.
- OpenClaw local lane: provider-auth dead-end removed; local lane now reaches model execution but the 60 second embedded probe timed out.

## Manual Generation Note

`/Users/dmcgregsauce/.codex/heiwa/bin/heiwax` is still unavailable in this environment, so this packet was generated manually to preserve the required Figma sync trail.
