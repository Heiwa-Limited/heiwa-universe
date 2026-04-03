# Canonical Heiwa Agents

Single authoring surface for shared Heiwa specialists.
See `docs/superpowers/specs/2026-03-29-cross-runtime-agent-canonicalization-design.md` for the design spec.

## Structure

Each agent lives in its own folder:
- `agent.yaml` — structured manifest with runtime targets
- `prompt.md` — canonical prompt body

## Commands

```bash
# Generate all runtime wrappers
uv run scripts/sync_agents.py

# Verify wrappers are current (CI candidate)
uv run scripts/sync_agents.py --check

# Install Codex wrappers into ~/.codex/skills/
uv run scripts/sync_agents.py --install-codex
```

## Rules

- Author prompts only in `ops/agents/<id>/prompt.md`
- Never hand-edit generated wrappers in `.gemini/agents/`, `.claude/agents/`, or `generated/codex/`
- Codex discovery is machine-global via `~/.codex/skills/`, but the source of truth remains this repo.
- Run `--check` before committing wrapper changes
