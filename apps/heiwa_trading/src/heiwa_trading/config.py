from __future__ import annotations

from os import environ
from pathlib import Path

from heiwa_trading.types import RiskPolicy


PROJECT_ROOT = Path(__file__).resolve().parents[2]
RUNTIME_DIR = PROJECT_ROOT / "runtime"
PORTFOLIO_PATH = RUNTIME_DIR / "paper_portfolio.json"
TOURNAMENT_STATE_PATH = RUNTIME_DIR / "tournament_state.json"
SCAN_HISTORY_PATH = RUNTIME_DIR / "scan_history.jsonl"
TUNING_REPORT_PATH = RUNTIME_DIR / "tuning_report.json"
COINMARKETCAP_CACHE_PATH = RUNTIME_DIR / "coinmarketcap_cache.json"
SUPERVISOR_STATE_PATH = RUNTIME_DIR / "supervisor_state.json"
MARKETS_BASE_URL = "https://gamma-api.polymarket.com/markets"
COINMARKETCAP_LISTINGS_URL = "https://pro-api.coinmarketcap.com/v1/cryptocurrency/listings/latest"
COINMARKETCAP_CACHE_TTL_SECONDS = 300
COINMARKETCAP_API_KEY_PATH = Path(environ.get(
    "COINMARKETCAP_API_KEY_FILE",
    str(Path(environ.get("HOME", str(Path.home()))) / ".heiwa" / "secrets" / "coinmarketcap_api_key"),
))
USER_AGENT = "heiwa-trading/0.1"
DEFAULT_TOURNAMENT_WALLETS = 10
DEFAULT_TOURNAMENT_LIMIT = 25
DEFAULT_TOURNAMENT_SLEEP_SECONDS = 60

CHEAP_POLICY = RiskPolicy(
    max_daily_loss=10.0,
    max_trade_notional=2.5,
    max_total_capital=50.0,
)
