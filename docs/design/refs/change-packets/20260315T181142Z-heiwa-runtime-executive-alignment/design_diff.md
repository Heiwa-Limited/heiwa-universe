# Design Diff

## Update

- Add an explicit "local execution substrate" layer beneath the existing Heiwa control-plane diagram.
- Show the Heiwa CLI launcher and `CLIContext` as the authority that exports the canonical repo root into downstream wrappers.
- Show the wrapper layer as a first-class routing boundary, not a transparent passthrough.
- Mark the Class 3 execution lanes with their actual runtime defaults:
  - Gemini: `yolo`
  - Claude: `bypassPermissions`
  - Codex: `danger-full-access`
- Show OpenClaw and PicoClaw as local agent runtimes aligned to the Heiwa repo root and local-first model ordering.

## Add

- A callout that wrapper fallback roots were corrected from nested `apps/heiwa_cli` / `apps` paths to the monorepo root.
- A callout that shell runtime exports now include the Ollama auth stub required by OpenClaw local mode.
- A note that `gate_build.py` now validates executive runtime defaults and wrapper-root correctness.
- A local model priority annotation for OpenClaw:
  - `qwen3.5:4b`
  - `qwen2.5-coder:1.5b`
  - `qwen2.5-coder:0.5b`
  - `llama3.2:3b`

## Remove

- Any visual that implies wrapper defaults are softer than the routing contract.
- Any visual that places the effective workspace root at `apps/heiwa_cli`.
- Any visual that shows PicoClaw reading identities from the deprecated `core/config/identity_profiles.json` path.
- Any visual that implies OpenClaw local mode is blocked by missing provider registration after this pass.
