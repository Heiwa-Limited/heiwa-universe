# Heiwa Finish Pass Design Diff

## What changed
- The CLI now has first-class provider auth orchestration for `gemini`, `codex`, `claude`, and `antigravity`, with provider metadata persisted into the Heiwa state layer.
- Inline approval is now part of the REPL and one-shot execution path. Risky tasks can pause in `waiting_approval`, resume the same run, or remain pending when interrupted.
- The state model now includes:
  - `provider_accounts`
  - `missions`
  - `mission_steps`
  - `cell_runs`
  - `session_summaries`
  - `artifacts`
- The hub now exposes authenticated operator endpoints for provider status, missions, rate groups, history, and an operator websocket snapshot.
- The web operator app now has dedicated pages:
  - `Connections`
  - `Mission Control`
  - `Live Run`
  - `Approvals`
  - `Rate Groups`
  - `Cells`
  - `History`

## Visual implications
- Add a dedicated operator information architecture distinct from the public shell.
- Reflect mission state progression explicitly:
  - `draft`
  - `clarifying`
  - `ready`
  - `running`
  - `waiting_approval`
  - `paused`
  - `completed`
  - `failed`
- Show rate reserve and surplus headroom as first-class values, not just raw usage.
- Provider cards should emphasize:
  - provider name
  - auth kind
  - connection status
  - last validated time
  - default model
  - last error

## Runtime truth to mirror in Figma
- The app is hub-served and browser-based, not a native desktop shell.
- Operator pages require the per-install hub token for data access.
- The operator websocket is snapshot-driven and reads:
  - providers
  - missions
  - approvals
  - live tasks
  - rate groups
  - history
- CLI remains the primary control surface; the app mirrors and controls the same backend state.
