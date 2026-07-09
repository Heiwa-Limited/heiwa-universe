# Agent skills & slash commands (builders)

These skills keep agents shipping **quality + progress** while building Heiwa with **any** inference stack (local, free OpenRouter/Nous, Claude Pro, ChatGPT Plus/Codex, SuperGrok, paid APIs).

## Grok Build

| Slash | Skill path |
|-------|------------|
| `/heiwa-progress` | `.grok/skills/heiwa-progress/SKILL.md` |
| `/heiwa-quality` | `.grok/skills/heiwa-quality/SKILL.md` |
| `/heiwa-inference` | `.grok/skills/heiwa-inference/SKILL.md` |
| `/heiwa-ship` | `.grok/skills/heiwa-ship/SKILL.md` |

Open the monorepo from WSL (`~/heiwa`) so repo-scoped skills load.

## Claude Code

Legacy commands: `.claude/commands/heiwa-*.md` → `/heiwa-progress`, etc.

## Codex / other agents

Read root `AGENTS.md` plus `.agents/skills/`. Nested package AGENTS.md when present.

## Product thesis for agents

Users should not need many apps/accounts for daily digital life—**except inference providers**, which remain theirs. Heiwa integrates life data on-device and lets **their** models power agency.
