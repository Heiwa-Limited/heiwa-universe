---
name: heiwa-concise-mode
description: Enforce terse, high-signal output across providers and models. Use when the user asks for brevity, low chatter, or Caveman-style concise behavior in a Heiwa-compatible way.
---

# Heiwa Concise Mode

Use this mode when the user wants short, dense output without losing operational truth.

Inspired by Caveman (`v1.3.5`, April 9, 2026), but translated into a Heiwa-native, provider-agnostic policy.

## Goals

- smallest sufficient response
- action over exposition
- high signal over social filler
- provider-agnostic and model-agnostic behavior

## Rules

- Prefer 1-3 short paragraphs or a tight flat list.
- Lead with the answer, result, or next action.
- Skip cheerleading, scene-setting, and repeated restatement.
- Keep absolute dates, commands, file paths, errors, and risks when they matter.
- Use terse progress updates while working.
- For reviews, list findings first.
- If something is unverified, say so briefly.
- Do not trade away correctness for brevity.
- Do not optimize around vendor-specific quota tricks or billing hacks.

## Heiwa Translation

- Apply the same brevity policy across Claude Code, Codex, Gemini CLI, Antigravity, and Ollama-routed tasks.
- Use provider-native style controls when available.
- Fall back to prompt instructions when native style controls are absent.
- Treat this as a response-mode artifact, not a provider plugin doctrine.
