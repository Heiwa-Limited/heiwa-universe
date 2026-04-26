# packages/heiwa_cognition — LLM Routing

Provides the unified LLM routing facade and execution layer for Heiwa's internal reasoning.

## Key Files

| File | Purpose |
| --- | --- |
| `llm.py` | `llm_generate*` facade + `LocalLLMEngine` execution layer |

## Routing Model

- `ComputeRouter` decides intent/risk/privacy/runtime routing
- `llm_generate()` and friends execute the routed inference plan
- `LocalLLMEngine` is call mechanics only; it no longer chooses provider chains

## Rules

- No paid API tiers unless the router explicitly selects one for the task class
- `GEMINI_API_KEY` is Google AI Studio free tier key
- Captain agent uses this facade for its own reasoning (event triage, proactive comms)
- Ollama tier only available when boost nodes are online
- CLI tools (Claude Code, Gemini CLI, Codex) are not routed through this facade for Class 3 execution; they are managed by HeiwaClaw / ToolMesh
