# cognition/ — Intent/Risk/Compute Pipeline

The enrichment pipeline that classifies user input and routes it to the right compute tier.

## Pipeline Flow

```
raw_text → IntentNormalizer → RiskScorer → ComputeRouter → BrokerRouteResult
```

## Files

| File | Purpose |
| --- | --- |
| `intent_normalizer.py` | Classifies user input into intent enums (build, deploy, research, audit, chat, etc.) |
| `risk_scorer.py` | Assigns risk level (low/medium/high/critical) based on intent + content analysis |
| `compute_router.py` | Maps (intent, risk) → compute class (1-4) + assigned worker |
| `planner.py` | Step decomposition from raw text into executable plan |

## Compute Classes

| Class | Name | Where | Examples |
| --- | --- | --- | --- |
| 1 | CPU-first | Local, ≤7B models | Chat, status checks |
| 2 | GPU-justified | Local, ≤32B models | Code gen, embeddings |
| 3 | Premium remote | Railway CLI tools + free APIs | Research, builds, reviews |
| 4 | Cloud persistence | Railway/STDB | Infra ops, state mutations |

## Rules

- Sovereign/private data must route to class 1 or 2 (never cloud)
- Intent classification is rule-based (no LLM inference in the pipeline itself)
- ComputeRouter reads `config/swarm/ai_router.json` for model registry
