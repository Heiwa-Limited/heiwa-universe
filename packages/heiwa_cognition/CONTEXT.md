# packages/heiwa_cognition — LLM Engine

Provides the LLM inference layer for Heiwa's internal reasoning.

## Key Files

| File | Purpose |
| --- | --- |
| `llm.py` | `LocalLLMEngine` — tiered LLM routing with automatic fallback |

## Tier Routing

| Tier | Provider | Use Case |
| --- | --- | --- |
| 1 | Gemini Flash (Google AI Studio, free) | Default for all internal reasoning |
| 2 | Gemini Pro (Google AI Studio, free) | Complex reasoning, longer context |
| 3 | Ollama (local, boost nodes only) | Offline/sovereign inference |

## Rules

- No paid API tiers — free APIs and subscription CLI tools only
- `GEMINI_API_KEY` is Google AI Studio free tier key
- Captain agent uses this for its own reasoning (event triage, proactive comms)
- Ollama tier only available when boost nodes are online
- CLI tools (Claude Code, Gemini CLI, Codex) are NOT routed through this engine — they are Class 3 executors managed by HeiwaClaw
