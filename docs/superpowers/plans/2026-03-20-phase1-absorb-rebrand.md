# Phase 1: Absorb & Rebrand — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the mac-agent trading cockpit into the Heiwa monorepo as `apps/heiwa_trading/`, rebrand to "Heiwa Trading," mount on the Hub's FastAPI, and clean up the home directory.

**Architecture:** The standalone mac-agent Python app (ThreadingHTTPServer + launchd daemons) becomes a FastAPI sub-application mounted on the existing Hub server. The cockpit UI is served as static files. The supervisor remains callable as a Python function but is no longer a daemon — it will be cron-triggered by the work loop in Phase 3. This phase makes no behavioral changes — same functionality, new home.

**Tech Stack:** Python 3.11, FastAPI, existing Hub (`apps/heiwa_hub/mcp_server.py`), static HTML/CSS/JS cockpit

**Spec:** `docs/superpowers/specs/2026-03-20-heiwa-work-loop-design.md`

---

## File Structure

### New files (created)

| File                                                    | Responsibility                                                      |
| ------------------------------------------------------- | ------------------------------------------------------------------- |
| `apps/heiwa_trading/__init__.py`                        | Package init                                                        |
| `apps/heiwa_trading/src/heiwa_trading/__init__.py`      | Module init                                                         |
| `apps/heiwa_trading/src/heiwa_trading/cli.py`           | CLI dispatcher (copied from polymarket_foundation, renamed imports) |
| `apps/heiwa_trading/src/heiwa_trading/cockpit.py`       | Cockpit state builder + SSE (copied, renamed imports)               |
| `apps/heiwa_trading/src/heiwa_trading/coinmarketcap.py` | CoinMarketCap data source (copied, renamed imports)                 |
| `apps/heiwa_trading/src/heiwa_trading/config.py`        | Paths, URLs, constants (copied, updated paths)                      |
| `apps/heiwa_trading/src/heiwa_trading/formulas.py`      | Pure math functions (copied unchanged)                              |
| `apps/heiwa_trading/src/heiwa_trading/market_data.py`   | Polymarket API ingestion (copied, renamed imports)                  |
| `apps/heiwa_trading/src/heiwa_trading/paper_trader.py`  | Portfolio simulation (copied, renamed imports)                      |
| `apps/heiwa_trading/src/heiwa_trading/scan.py`          | Market scanning (copied, renamed imports)                           |
| `apps/heiwa_trading/src/heiwa_trading/strategy.py`      | Scoring engine (copied, renamed imports)                            |
| `apps/heiwa_trading/src/heiwa_trading/supervisor.py`    | Supervisor logic (copied, renamed imports)                          |
| `apps/heiwa_trading/src/heiwa_trading/tournament.py`    | Cohort + variant management (copied, renamed imports)               |
| `apps/heiwa_trading/src/heiwa_trading/types.py`         | Frozen dataclasses (copied unchanged)                               |
| `apps/heiwa_trading/src/heiwa_trading/web/cockpit.html` | Cockpit UI (copied, rebranded)                                      |
| `apps/heiwa_trading/src/heiwa_trading/web/cockpit.css`  | Cockpit styles (copied unchanged)                                   |
| `apps/heiwa_trading/src/heiwa_trading/web/cockpit.js`   | Cockpit JS (copied, rebranded)                                      |
| `apps/heiwa_trading/src/heiwa_trading/routes.py`        | FastAPI router for trading endpoints                                |
| `apps/heiwa_trading/pyproject.toml`                     | Package metadata                                                    |
| `apps/heiwa_trading/CONTEXT.md`                         | Agent context for this app                                          |
| `apps/heiwa_trading/tests/__init__.py`                  | Test package init                                                   |
| `apps/heiwa_trading/tests/test_formulas.py`             | Unit tests for formulas                                             |
| `apps/heiwa_trading/tests/test_strategy.py`             | Unit tests for scoring engine                                       |
| `apps/heiwa_trading/tests/test_routes.py`               | Integration tests for FastAPI routes                                |
| `apps/heiwa_dj/README.md`                               | Archive pointer for shipped AI-DJ                                   |
| `apps/heiwa_dj/CONTEXT.md`                              | Agent context for archived app                                      |

### Modified files

| File                           | Change                                       |
| ------------------------------ | -------------------------------------------- |
| `apps/heiwa_hub/mcp_server.py` | Mount trading router                         |
| `HEIWA.md`                     | Update app directory table                   |
| `requirements.txt`             | No new deps needed (FastAPI already present) |

### Files NOT copied (mac-agent specific, not needed)

| File                 | Reason                                              |
| -------------------- | --------------------------------------------------- |
| `cockpit_service.py` | LaunchAgent installer — Railway doesn't use launchd |
| `cron_jobs.py`       | Will be replaced by work loop cron in Phase 3       |
| `live_dashboard.py`  | Terminal UI — replaced by web cockpit               |
| `evolution.py`       | Deferred to Phase 5 (autoresearch for trading)      |

**Note on `web/` location:** The spec shows `web/` at `apps/heiwa_trading/web/`, but this plan nests it at `apps/heiwa_trading/src/heiwa_trading/web/` so that `Path(__file__).parent / "web"` resolves correctly in `routes.py`. This is an intentional deviation for practical import reasons.

---

## Chunk 1: Copy, Rename & Verify Imports

### Task 1: Create heiwa_trading package structure

**Files:**

- Create: `apps/heiwa_trading/__init__.py`
- Create: `apps/heiwa_trading/pyproject.toml`
- Create: `apps/heiwa_trading/src/heiwa_trading/__init__.py`

- [ ] **Step 1: Create directory structure**

```bash
mkdir -p apps/heiwa_trading/src/heiwa_trading/web
mkdir -p apps/heiwa_trading/tests
mkdir -p apps/heiwa_trading/runtime
```

- [ ] **Step 2: Create package init files**

`apps/heiwa_trading/__init__.py`:

```python
```

`apps/heiwa_trading/src/heiwa_trading/__init__.py`:

```python
"""Heiwa Trading — Polymarket paper-trading tournament engine."""
```

`apps/heiwa_trading/tests/__init__.py`:

```python
```

- [ ] **Step 3: Create pyproject.toml**

`apps/heiwa_trading/pyproject.toml`:

```toml
[project]
name = "heiwa-trading"
version = "0.1.0"
description = "Polymarket paper-trading tournament engine for Heiwa."
requires-python = ">=3.11"

[tool.pytest.ini_options]
testpaths = ["tests"]
```

- [ ] **Step 4: Add runtime/ to .gitignore**

Append to the repo's `.gitignore`:

```
apps/heiwa_trading/runtime/
```

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_trading/ .gitignore
git commit -m "feat(trading): scaffold heiwa_trading package structure"
```

### Task 2: Copy and rename core modules

**Files:**

- Create: `apps/heiwa_trading/src/heiwa_trading/types.py` (from `polymarket_foundation/types.py`)
- Create: `apps/heiwa_trading/src/heiwa_trading/formulas.py` (from `polymarket_foundation/formulas.py`)
- Create: `apps/heiwa_trading/src/heiwa_trading/config.py` (from `polymarket_foundation/config.py`)

These three files form the dependency base — everything else imports from them.

- [ ] **Step 1: Copy types.py unchanged**

```bash
cp ~/mac-agent/workspace/polymarket_foundation/src/polymarket_foundation/types.py \
   apps/heiwa_trading/src/heiwa_trading/types.py
```

This file has no internal imports — copy as-is.

- [ ] **Step 2: Copy formulas.py unchanged**

```bash
cp ~/mac-agent/workspace/polymarket_foundation/src/polymarket_foundation/formulas.py \
   apps/heiwa_trading/src/heiwa_trading/formulas.py
```

This file has no internal imports — copy as-is.

- [ ] **Step 3: Copy and update config.py**

```bash
cp ~/mac-agent/workspace/polymarket_foundation/src/polymarket_foundation/config.py \
   apps/heiwa_trading/src/heiwa_trading/config.py
```

Then edit `apps/heiwa_trading/src/heiwa_trading/config.py`:

Replace:

```python
from polymarket_foundation.types import RiskPolicy
```

With:

```python
from heiwa_trading.types import RiskPolicy
```

Replace:

```python
USER_AGENT = "mac-agent-polymarket/0.1"
```

With:

```python
USER_AGENT = "heiwa-trading/0.1"
```

Replace:

```python
COINMARKETCAP_API_KEY_PATH = Path(environ.get("HOME", str(Path.home()))) / ".mac-agent" / "home" / "secrets" / "coinmarketcap_api_key"
```

With:

```python
COINMARKETCAP_API_KEY_PATH = Path(environ.get(
    "COINMARKETCAP_API_KEY_FILE",
    str(Path(environ.get("HOME", str(Path.home()))) / ".heiwa" / "secrets" / "coinmarketcap_api_key"),
))
```

- [ ] **Step 4: Verify imports parse**

```bash
cd apps/heiwa_trading
PYTHONPATH=src python -c "from heiwa_trading.types import RiskPolicy; print('types OK')"
PYTHONPATH=src python -c "from heiwa_trading.formulas import kelly_fraction; print('formulas OK')"
PYTHONPATH=src python -c "from heiwa_trading.config import CHEAP_POLICY; print('config OK')"
```

Expected: all three print OK.

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_trading/src/heiwa_trading/types.py \
        apps/heiwa_trading/src/heiwa_trading/formulas.py \
        apps/heiwa_trading/src/heiwa_trading/config.py
git commit -m "feat(trading): add core modules (types, formulas, config)"
```

### Task 3: Copy and rename remaining modules

**Files:**

- Create: all remaining `.py` files in `apps/heiwa_trading/src/heiwa_trading/`

- [ ] **Step 1: Copy all remaining source files**

```bash
for f in market_data.py strategy.py paper_trader.py scan.py tournament.py \
         supervisor.py cockpit.py coinmarketcap.py cli.py; do
  cp ~/mac-agent/workspace/polymarket_foundation/src/polymarket_foundation/$f \
     apps/heiwa_trading/src/heiwa_trading/$f
done
```

- [ ] **Step 2: Rename all imports from polymarket_foundation → heiwa_trading**

In every `.py` file in `apps/heiwa_trading/src/heiwa_trading/`, replace:

```python
from polymarket_foundation.
```

With:

```python
from heiwa_trading.
```

And replace:

```python
import polymarket_foundation.
```

With:

```python
import heiwa_trading.
```

Use sed or manual edit — verify every file.

- [ ] **Step 2b: Update cockpit.py hardcoded mac-agent paths**

In `apps/heiwa_trading/src/heiwa_trading/cockpit.py`, update these constants:

Replace:

```python
LOG_DIR = Path.home() / ".mac-agent" / "openclaw" / "logs"
OPENCLAW_STATE_DIR = Path.home() / ".mac-agent" / "openclaw"
```

With:

```python
LOG_DIR = Path.home() / ".heiwa" / "logs"
OPENCLAW_STATE_DIR = Path.home() / ".heiwa" / "openclaw"
```

The `MAC_AGENT_ROOT` constant (set to `PROJECT_ROOT.parents[1]`) resolves correctly in the new location — no change needed. The `ThreadingHTTPServer` class and `CockpitServerConfig` are unused when served via FastAPI routes — leave them in place for now (they'll be cleaned up when the cockpit is fully Railway-hosted).

- [ ] **Step 3: Copy web assets**

```bash
cp ~/mac-agent/workspace/polymarket_foundation/src/polymarket_foundation/web/cockpit.html \
   apps/heiwa_trading/src/heiwa_trading/web/cockpit.html
cp ~/mac-agent/workspace/polymarket_foundation/src/polymarket_foundation/web/cockpit.css \
   apps/heiwa_trading/src/heiwa_trading/web/cockpit.css
cp ~/mac-agent/workspace/polymarket_foundation/src/polymarket_foundation/web/cockpit.js \
   apps/heiwa_trading/src/heiwa_trading/web/cockpit.js
```

- [ ] **Step 4: Verify all imports parse**

```bash
cd apps/heiwa_trading
PYTHONPATH=src python -c "
from heiwa_trading import types, formulas, config
from heiwa_trading import market_data, strategy, paper_trader
from heiwa_trading import scan, tournament, supervisor
from heiwa_trading import cockpit, coinmarketcap
print('All imports OK')
"
```

Expected: `All imports OK`

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_trading/src/heiwa_trading/
git commit -m "feat(trading): copy and rename all modules from mac-agent"
```

## Chunk 2: Rebrand UI & Write Tests

### Task 4: Rebrand cockpit UI

**Files:**

- Modify: `apps/heiwa_trading/src/heiwa_trading/web/cockpit.html`
- Modify: `apps/heiwa_trading/src/heiwa_trading/web/cockpit.js`

- [ ] **Step 1: Rebrand cockpit.html**

In `apps/heiwa_trading/src/heiwa_trading/web/cockpit.html`:

Replace all occurrences of:

- `Mac Agent` → `Heiwa Trading`
- `mac-agent` → `heiwa-trading`
- `Mac-Agent` → `Heiwa-Trading`

Update the `<title>` tag:

```html
<title>Heiwa Trading</title>
```

- [ ] **Step 2: Rebrand cockpit.js**

In `apps/heiwa_trading/src/heiwa_trading/web/cockpit.js`:

Replace all occurrences of:

- `Mac Agent` → `Heiwa Trading`
- `mac-agent` → `heiwa-trading`

- [ ] **Step 3: Visual verification (local)**

Open `apps/heiwa_trading/src/heiwa_trading/web/cockpit.html` in a browser. Verify:

- Title says "Heiwa Trading"
- Hero banner says "Heiwa Trading"
- No remaining "Mac Agent" text visible

- [ ] **Step 4: Commit**

```bash
git add apps/heiwa_trading/src/heiwa_trading/web/
git commit -m "feat(trading): rebrand cockpit UI to Heiwa Trading"
```

### Task 5: Write unit tests for formulas and strategy

**Files:**

- Create: `apps/heiwa_trading/tests/test_formulas.py`
- Create: `apps/heiwa_trading/tests/test_strategy.py`

- [ ] **Step 1: Write formulas tests**

`apps/heiwa_trading/tests/test_formulas.py`:

```python
"""Tests for heiwa_trading.formulas — pure math, no side effects."""
import pytest
from heiwa_trading.formulas import kelly_fraction, expected_value, log_odds_edge


def test_kelly_fraction_positive_edge():
    """Kelly should return positive fraction when edge > 0."""
    result = kelly_fraction(probability=0.7, price=0.4)
    assert result > 0.0
    assert result < 1.0


def test_kelly_fraction_no_edge():
    """Kelly should return 0 when no edge (probability == price)."""
    result = kelly_fraction(probability=0.5, price=0.5)
    assert result == 0.0


def test_expected_value_positive():
    result = expected_value(probability=0.7, price=0.4)
    assert result > 0.0


def test_expected_value_negative():
    result = expected_value(probability=0.2, price=0.6)
    assert result < 0.0


def test_log_odds_edge_symmetric():
    """Equal probabilities should yield zero edge."""
    result = log_odds_edge(subjective_probability=0.5, market_probability=0.5)
    assert abs(result) < 1e-9
```

- [ ] **Step 2: Run formulas tests to verify they pass**

```bash
cd apps/heiwa_trading
PYTHONPATH=src pytest tests/test_formulas.py -v
```

Expected: All tests pass. If any function names are wrong, check `formulas.py` and adjust test imports.

- [ ] **Step 3: Write strategy tests**

`apps/heiwa_trading/tests/test_strategy.py`:

```python
"""Tests for heiwa_trading.strategy — scoring engine."""
import pytest
from heiwa_trading.types import NormalizedMarket, RiskPolicy, ScoreDecision
from heiwa_trading.strategy import score_market
from heiwa_trading.config import CHEAP_POLICY


def _make_market(**overrides) -> NormalizedMarket:
    defaults = dict(
        market_id="test-123",
        slug="test-market",
        question="Will it rain?",
        yes_price=0.6,
        no_price=0.4,
        liquidity=5000.0,
        volume_24hr=10000.0,
        active=True,
        closed=False,
        enable_order_book=True,
    )
    defaults.update(overrides)
    return NormalizedMarket(**defaults)


def test_score_market_returns_score_decision():
    """score_market should return a ScoreDecision dataclass."""
    market = _make_market()
    result = score_market(
        market=market,
        subjective_probability=0.7,
        policy=CHEAP_POLICY,
    )
    assert isinstance(result, ScoreDecision)
    assert hasattr(result, "expected_value")
    assert hasattr(result, "kelly_fraction")
    assert hasattr(result, "action")


def test_score_market_skips_inactive():
    """Inactive markets should get SKIP action."""
    market = _make_market(active=False)
    result = score_market(
        market=market,
        subjective_probability=0.7,
        policy=CHEAP_POLICY,
    )
    assert result.action == "SKIP"
```

- [ ] **Step 4: Run strategy tests**

```bash
cd apps/heiwa_trading
PYTHONPATH=src pytest tests/test_strategy.py -v
```

Expected: Pass (adjust field names if the actual function signature differs).

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_trading/tests/
git commit -m "test(trading): add unit tests for formulas and strategy"
```

## Chunk 3: FastAPI Integration

### Task 6: Create FastAPI trading router

**Files:**

- Create: `apps/heiwa_trading/src/heiwa_trading/routes.py`

- [ ] **Step 1: Write the routes module**

`apps/heiwa_trading/src/heiwa_trading/routes.py`:

```python
"""FastAPI routes for Heiwa Trading cockpit.

Mounted on the Hub at /trading/*.
"""
from __future__ import annotations

import asyncio
import json
import logging
from pathlib import Path
from typing import Any

from fastapi import APIRouter, Request
from fastapi.responses import FileResponse, HTMLResponse, StreamingResponse

from heiwa_trading.cockpit import (
    build_cockpit_snapshot,
    append_cockpit_chat_message,
)
from heiwa_trading.supervisor import supervisor_tick, load_supervisor_state
from heiwa_trading.market_data import fetch_markets

logger = logging.getLogger("Trading.Routes")

router = APIRouter(prefix="/trading", tags=["trading"])

WEB_DIR = Path(__file__).parent / "web"


@router.get("/cockpit")
async def cockpit_page():
    """Serve the trading cockpit HTML."""
    return FileResponse(WEB_DIR / "cockpit.html", media_type="text/html")


@router.get("/cockpit.css")
async def cockpit_css():
    return FileResponse(WEB_DIR / "cockpit.css", media_type="text/css")


@router.get("/cockpit.js")
async def cockpit_js():
    return FileResponse(WEB_DIR / "cockpit.js", media_type="application/javascript")


@router.get("/api/state")
async def trading_state():
    """Return current supervisor state as JSON."""
    snapshot = build_cockpit_snapshot()
    return snapshot


@router.post("/api/action")
async def trading_action(request: Request):
    """Handle operator control actions (tick, init cohort, etc.)."""
    body = await request.json()
    action = body.get("action", "")
    result: dict[str, object] = {"status": "ok", "action": action}

    if action == "tick":
        from datetime import datetime, timezone
        state = load_supervisor_state()
        markets = fetch_markets()
        ts = datetime.now(timezone.utc).isoformat()
        new_state, summary = supervisor_tick(state=state, markets=markets, timestamp=ts)
        result["message"] = "Tick completed"
        result["summary"] = summary
    elif action == "chat":
        text = body.get("text", "")
        append_cockpit_chat_message(role="operator", text=text)
        result["message"] = f"Chat: {text}"
    else:
        result["status"] = "unknown_action"

    return result


@router.get("/sse")
async def trading_sse():
    """SSE stream pushing cockpit state updates."""
    async def event_generator():
        while True:
            snapshot = build_cockpit_snapshot()
            data = json.dumps(snapshot)
            yield f"data: {data}\n\n"
            await asyncio.sleep(3)

    return StreamingResponse(
        event_generator(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "Connection": "keep-alive",
        },
    )
```

- [ ] **Step 2: Commit**

```bash
git add apps/heiwa_trading/src/heiwa_trading/routes.py
git commit -m "feat(trading): add FastAPI trading router"
```

### Task 7: Mount trading router on Hub

**Files:**

- Modify: `apps/heiwa_hub/mcp_server.py`

- [ ] **Step 1: Add trading router import and mount**

In `apps/heiwa_hub/mcp_server.py`, after the existing imports (around line 35), add:

```python
try:
    from heiwa_trading.routes import router as trading_router
    app.include_router(trading_router)
    logger.info("Trading routes mounted at /trading/*")
except ImportError:
    logger.info("heiwa_trading not available, trading routes disabled")
```

The try/except ensures the Hub still boots if heiwa_trading is not on the Python path (e.g., during CI where only hub tests run).

- [ ] **Step 2: Update PYTHONPATH in CLAUDE.md setup section**

In the repo's `CLAUDE.md`, update the PYTHONPATH export to include the trading app:

Add `$(pwd)/apps/heiwa_trading/src` to the existing PYTHONPATH line.

- [ ] **Step 3: Update PYTHONPATH in start.sh**

In `apps/heiwa_hub/start.sh` (line 187), replace:

```bash
export PYTHONPATH="/app/packages/heiwa_cli:/app/packages/heiwa_cognition:/app/packages/heiwa_sdk:/app/packages/heiwa_protocol:/app/packages/heiwa_identity:/app/packages/heiwa_ui:/app/apps:${PYTHONPATH:-}"
```

With:

```bash
export PYTHONPATH="/app/packages/heiwa_cli:/app/packages/heiwa_cognition:/app/packages/heiwa_sdk:/app/packages/heiwa_protocol:/app/packages/heiwa_identity:/app/packages/heiwa_ui:/app/apps:/app/apps/heiwa_trading/src:${PYTHONPATH:-}"
```

- [ ] **Step 4: Verify Hub boots with trading routes**

```bash
export PYTHONPATH="$(pwd)/packages/heiwa_cli:$(pwd)/packages/heiwa_cognition:$(pwd)/packages/heiwa_sdk:$(pwd)/packages/heiwa_protocol:$(pwd)/packages/heiwa_identity:$(pwd)/packages/heiwa_ui:$(pwd)/apps:$(pwd)/apps/heiwa_trading/src"
timeout 5 python -m apps.heiwa_hub.main 2>&1 | grep -i "trading\|started\|error" || true
```

Expected: See "Trading routes mounted at /trading/*" in output. Hub may fail on STDB connection or auth — that's fine, we're only checking the import works.

- [ ] **Step 5: Commit**

```bash
git add apps/heiwa_hub/mcp_server.py apps/heiwa_hub/start.sh CLAUDE.md
git commit -m "feat(hub): mount trading router on Hub FastAPI"
```

### Task 8: Write integration tests for trading routes

**Files:**

- Create: `apps/heiwa_trading/tests/test_routes.py`

- [ ] **Step 1: Write route tests**

`apps/heiwa_trading/tests/test_routes.py`:

```python
"""Integration tests for heiwa_trading FastAPI routes."""
import pytest
from fastapi.testclient import TestClient
from fastapi import FastAPI

from heiwa_trading.routes import router


@pytest.fixture
def client():
    app = FastAPI()
    app.include_router(router)
    return TestClient(app)


def test_cockpit_page_returns_html(client):
    response = client.get("/trading/cockpit")
    assert response.status_code == 200
    assert "text/html" in response.headers["content-type"]
    assert "Heiwa Trading" in response.text


def test_cockpit_css_returns_css(client):
    response = client.get("/trading/cockpit.css")
    assert response.status_code == 200
    assert "text/css" in response.headers["content-type"]


def test_cockpit_js_returns_js(client):
    response = client.get("/trading/cockpit.js")
    assert response.status_code == 200
    assert "javascript" in response.headers["content-type"]


def test_trading_state_returns_json(client):
    response = client.get("/trading/api/state")
    assert response.status_code == 200
    data = response.json()
    assert isinstance(data, dict)


def test_cockpit_no_mac_agent_branding(client):
    """Verify all Mac Agent branding has been removed."""
    response = client.get("/trading/cockpit")
    text = response.text.lower()
    assert "mac agent" not in text
    assert "mac-agent" not in text
```

- [ ] **Step 2: Run route tests**

```bash
cd apps/heiwa_trading
PYTHONPATH=src pytest tests/test_routes.py -v
```

Expected: All pass. The `test_trading_state_returns_json` test may need adjustment if `build_cockpit_snapshot()` requires runtime state files — if so, mock the state file or create a minimal one in the test fixture.

- [ ] **Step 3: Commit**

```bash
git add apps/heiwa_trading/tests/test_routes.py
git commit -m "test(trading): add integration tests for FastAPI routes"
```

## Chunk 4: Context Docs, Archive Pointer & Cleanup

### Task 9: Write CONTEXT.md for heiwa_trading

**Files:**

- Create: `apps/heiwa_trading/CONTEXT.md`

- [ ] **Step 1: Write CONTEXT.md**

`apps/heiwa_trading/CONTEXT.md`:

````markdown
# Heiwa Trading — Context

## What This Is

Polymarket paper-trading tournament engine. Scans prediction markets, scores opportunities using EV/Kelly/Bayes/log-odds formulas, runs 10-wallet cohort tournaments with strategy variants, and serves a browser-based operator cockpit.

## Key Files

| File               | Purpose                                                 |
| ------------------ | ------------------------------------------------------- |
| `routes.py`        | FastAPI router mounted on Hub at `/trading/*`           |
| `supervisor.py`    | Market supervisor — called as a function, not a daemon  |
| `cockpit.py`       | State builder, SSE helper functions                     |
| `strategy.py`      | Scoring engine (EV, Kelly, Bayes, log-odds)             |
| `formulas.py`      | Pure math functions                                     |
| `market_data.py`   | Polymarket public API ingestion                         |
| `paper_trader.py`  | Paper portfolio simulation with risk policy             |
| `tournament.py`    | Cohort + strategy variant management                    |
| `coinmarketcap.py` | CoinMarketCap movers data (cached)                      |
| `types.py`         | Frozen dataclasses (RiskPolicy, NormalizedMarket, etc.) |
| `config.py`        | Paths, URLs, constants                                  |
| `web/`             | Static cockpit UI (HTML/CSS/JS)                         |

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
````

````
- [ ] **Step 2: Commit**

```bash
git add apps/heiwa_trading/CONTEXT.md
git commit -m "docs(trading): add CONTEXT.md for heiwa_trading"
````

### Task 10: Create AI-DJ archive pointer

**Files:**

- Create: `apps/heiwa_dj/README.md`
- Create: `apps/heiwa_dj/CONTEXT.md`

- [ ] **Step 1: Create directory**

```bash
mkdir -p apps/heiwa_dj
```

- [ ] **Step 2: Write README.md**

`apps/heiwa_dj/README.md`:

```markdown
# Heiwa DJ — Archived

Shipped as v1.7.0. Standalone Electron app lives at `~/ai-dj/`.

This directory is an archive pointer only — no code lives here. If the Captain needs to work on AI-DJ (bug fix, feature request), the standalone repo is the source of truth.

## Quick Reference

- **Stack:** Express.js + Ollama (qwen3.5:4b) + Strudel live-coding engine
- **Build:** `cd ~/ai-dj && pnpm desktop:build`
- **Run:** `cd ~/ai-dj && pnpm desktop:dev`
- **Artifact:** `~/ai-dj/packages/desktop/release/Heiwa-DJ-1.7.0-arm64.dmg`
```

- [ ] **Step 3: Write CONTEXT.md**

`apps/heiwa_dj/CONTEXT.md`:

```markdown
# Heiwa DJ — Context

Shipped product (v1.7.0). Standalone Electron app at `~/ai-dj/`.

This is an archive pointer. The code does not live in the monorepo. See `~/ai-dj/` for the full codebase, or `README.md` in this directory for quick reference.
```

- [ ] **Step 4: Commit**

```bash
git add apps/heiwa_dj/
git commit -m "docs: add heiwa_dj archive pointer"
```

### Task 11: Update HEIWA.md app directory table

**Files:**

- Modify: `HEIWA.md`

- [ ] **Step 1: Update the Directory Context Files table**

In `HEIWA.md`, add entries to the "Directory Context Files" table:

```markdown
| `apps/heiwa_trading/` | `CONTEXT.md` | Trading cockpit, supervisor, strategy engine |
| `apps/heiwa_dj/` | `CONTEXT.md` | Archived — shipped v1.7.0 standalone app |
```

- [ ] **Step 2: Commit**

```bash
git add HEIWA.md
git commit -m "docs: add heiwa_trading and heiwa_dj to HEIWA.md directory table"
```

### Task 12: Update home directory configs

**Files:**

- Modify: `~/CLAUDE.md`
- Modify: `~/.gemini/GEMINI.md`
- Modify: `~/.codex/AGENTS.md`

- [ ] **Step 1: Update project landscape tables**

In all three files, update the mac-agent row to reflect it has been absorbed:

Change:

```
| `~/mac-agent/` | Polymarket paper-trading tournament — ...  | **Active, running** |
```

To:

```
| `~/heiwa/apps/heiwa_trading/` | Polymarket paper-trading tournament — absorbed into monorepo | **Active, in monorepo** |
```

- [ ] **Step 2: Commit the config changes (outside monorepo)**

These files are outside the Heiwa repo, so commit separately or just save them.

### Task 13: Clean up home directory

- [ ] **Step 1: Verify mac-agent code is committed in monorepo**

```bash
cd ~/heiwa
git log --oneline -5  # verify recent trading commits
ls apps/heiwa_trading/src/heiwa_trading/*.py | wc -l  # should be 12+
```

- [ ] **Step 2: Move CoinMarketCap API key to new location (BEFORE deleting mac-agent)**

```bash
mkdir -p ~/.heiwa/secrets
cp ~/.mac-agent/home/secrets/coinmarketcap_api_key ~/.heiwa/secrets/coinmarketcap_api_key 2>/dev/null || true
```

- [ ] **Step 3: Delete mac-agent (only after verification and key copy)**

```bash
rm -rf ~/mac-agent
rm -rf ~/.mac-agent
```

- [ ] **Step 4: Delete stray files**

```bash
rm -f ~/hub.db
rm -rf ~/bitcrap
rm -rf ~/R&D
```

- [ ] **Step 5: Verify home directory is clean**

```bash
ls ~/
```

Expected: `heiwa`, `ai-dj`, `heiwa_archive`, plus standard macOS dirs. No `mac-agent`, `bitcrap`, `R&D`, `hub.db`.

### Task 14: Verify Railway deployment

- [ ] **Step 1: Run all trading tests locally**

```bash
cd ~/heiwa/apps/heiwa_trading
PYTHONPATH=src pytest tests/ -v
```

Expected: All pass.

- [ ] **Step 2: Run Hub smoke tests**

```bash
cd ~/heiwa
pytest apps/heiwa_hub/tests/ -v
```

Expected: All existing tests still pass (no regressions).

- [ ] **Step 3: Push to main and verify Railway deploys**

```bash
git push origin main
```

Monitor Railway dashboard for successful deploy. Verify `/trading/cockpit` is accessible on the Railway URL.

- [ ] **Step 4: Final commit — update status**

If everything works, update the spec status:

In `docs/superpowers/specs/2026-03-20-heiwa-work-loop-design.md`, change Phase 1 status to indicate completion.

```bash
git add docs/superpowers/specs/2026-03-20-heiwa-work-loop-design.md
git commit -m "docs: mark Phase 1 (Absorb & Rebrand) complete"
```
