---
name: heiwa-inference
description: Choose and configure inference for Heiwa build work and for end-user BYOK (OpenRouter, Nous, Ollama, Claude, Codex, Grok). Use when setting models, keys, routing, or MCP gateway.
argument-hint: "[task: setup|route|keys|gateway]"
user-invocable: true
---

# /heiwa-inference — multi-provider efficiency

## Builder (Devon heavy node)

| Task class | Prefer |
|------------|--------|
| Boilerplate / format / small tests | Local Ollama or free OpenRouter |
| Feature implementation mid complexity | Claude Pro / Codex / SuperGrok session |
| Architecture / security | Strongest available subscription |
| Batch embedding / classify | Local embeddings |

Keys live in `~/heiwa/.env` (never commit). MCP gateway: `services/mcp-gateway`.

## End users (product)

Heiwa must make **their** keys and CLIs first-class:

- `heiwa auth add-key <provider> <key>`
- `heiwa auth login <provider>` for OAuth CLIs
- Local Ollama discovery
- OpenRouter as multi-model BYOK hub
- Never require Heiwa-hosted inference as the only path

## Route string convention

`provider/model` e.g. `openrouter/anthropic/claude-sonnet-4`, `ollama/qwen3.5:4b`, `xai/grok-3`.

$ARGUMENTS
