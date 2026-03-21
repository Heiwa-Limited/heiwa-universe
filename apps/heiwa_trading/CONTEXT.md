# Heiwa Trading — Context

## What This Is

Polymarket paper-trading tournament engine. Scans prediction markets, scores opportunities using EV/Kelly/Bayes/log-odds formulas, runs 10-wallet cohort tournaments with strategy variants, and serves a browser-based operator cockpit.

## Key Files

| File | Purpose |
|------|---------|
| `routes.py` | FastAPI router mounted on Hub at `/trading/*` |
| `supervisor.py` | Market supervisor — called as a function, not a daemon |
| `cockpit.py` | State builder, SSE helper functions |
| `strategy.py` | Scoring engine (EV, Kelly, Bayes, log-odds) |
| `formulas.py` | Pure math functions |
| `market_data.py` | Polymarket public API ingestion |
| `paper_trader.py` | Paper portfolio simulation with risk policy |
| `tournament.py` | Cohort + strategy variant management |
| `coinmarketcap.py` | CoinMarketCap movers data (cached) |
| `types.py` | Frozen dataclasses (RiskPolicy, NormalizedMarket, etc.) |
| `config.py` | Paths, URLs, constants |
| `web/` | Static cockpit UI (HTML/CSS/JS) |

## How It Runs

The cockpit is served via the Hub's FastAPI at `/trading/cockpit`. The supervisor is called as a Python function — in Phase 3 this becomes a cron-triggered WorkItem in Heiwa's work loop.

## State

Currently: JSON files in `runtime/` (gitignored).
Phase 2: Migrates to SpacetimeDB tables.

## Tests

```bash
cd apps/heiwa_trading
PYTHONPATH=src pytest tests/ -v
```
