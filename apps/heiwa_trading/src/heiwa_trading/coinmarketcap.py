from __future__ import annotations

import json
import os
import time
from pathlib import Path
from typing import Any, Mapping
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from heiwa_trading.config import (
    COINMARKETCAP_API_KEY_PATH,
    COINMARKETCAP_CACHE_PATH,
    COINMARKETCAP_CACHE_TTL_SECONDS,
    COINMARKETCAP_LISTINGS_URL,
    USER_AGENT,
)


def load_coinmarketcap_api_key(
    *,
    env: Mapping[str, str] | None = None,
    path: Path = COINMARKETCAP_API_KEY_PATH,
) -> str | None:
    env = os.environ if env is None else env
    for name in ("COINMARKETCAP_API_KEY", "CMC_API_KEY"):
        value = env.get(name)
        if value:
            return value.strip()
    if path.exists():
        value = path.read_text(encoding="utf-8").strip()
        return value or None
    return None


def summarize_coinmarketcap_listings(payload: dict[str, Any], *, mover_limit: int = 5) -> dict[str, object]:
    assets: list[dict[str, object]] = []
    for item in payload.get("data", []):
        quote = item.get("quote", {}).get("USD", {})
        assets.append(
            {
                "symbol": item.get("symbol"),
                "name": item.get("name"),
                "slug": item.get("slug"),
                "rank": item.get("cmc_rank"),
                "price": round(float(quote.get("price", 0.0)), 6),
                "market_cap": round(float(quote.get("market_cap", 0.0)), 2),
                "volume_24h": round(float(quote.get("volume_24h", 0.0)), 2),
                "percent_change_1h": round(float(quote.get("percent_change_1h", 0.0)), 3),
                "percent_change_24h": round(float(quote.get("percent_change_24h", 0.0)), 3),
                "percent_change_7d": round(float(quote.get("percent_change_7d", 0.0)), 3),
            }
        )

    top_movers = sorted(assets, key=lambda asset: abs(float(asset["percent_change_24h"])), reverse=True)[:mover_limit]
    top_market_cap = sorted(assets, key=lambda asset: float(asset["market_cap"]), reverse=True)[:mover_limit]
    return {
        "status": "ok",
        "message": "Fetched CoinMarketCap listings.",
        "asset_count": len(assets),
        "as_of": payload.get("status", {}).get("timestamp"),
        "top_movers_24h": top_movers,
        "top_market_cap": top_market_cap,
        "cached": False,
    }


def _request_coinmarketcap_listings(api_key: str, *, limit: int) -> dict[str, Any]:
    params = urlencode({"start": 1, "limit": limit, "convert": "USD"})
    request = Request(
        f"{COINMARKETCAP_LISTINGS_URL}?{params}",
        headers={
            "Accepts": "application/json",
            "User-Agent": USER_AGENT,
            "X-CMC_PRO_API_KEY": api_key,
        },
    )
    with urlopen(request, timeout=10) as response:
        return json.loads(response.read().decode("utf-8"))


def fetch_coinmarketcap_summary(
    *,
    api_key: str | None = None,
    limit: int = 25,
    mover_limit: int = 5,
    cache_path: Path = COINMARKETCAP_CACHE_PATH,
    ttl_seconds: int = COINMARKETCAP_CACHE_TTL_SECONDS,
    now: float | None = None,
    fetcher: callable | None = None,
) -> dict[str, object]:
    now = time.time() if now is None else now
    api_key = api_key or load_coinmarketcap_api_key()
    if cache_path.exists():
        cached_payload = json.loads(cache_path.read_text(encoding="utf-8"))
        fetched_at = float(cached_payload.get("fetched_at", 0.0))
        cached_summary = dict(cached_payload.get("summary", {}))
        if fetched_at and now - fetched_at < ttl_seconds:
            cached_summary["cached"] = True
            return cached_summary

    if not api_key:
        return {
            "status": "pending_config",
            "message": "CoinMarketCap API key is not configured.",
            "asset_count": 0,
            "top_movers_24h": [],
            "top_market_cap": [],
            "cached": False,
        }

    fetcher = _request_coinmarketcap_listings if fetcher is None else fetcher
    try:
        payload = fetcher(api_key, limit=limit)
    except Exception as exc:  # pragma: no cover - exercised through command verification
        return {
            "status": "error",
            "message": f"CoinMarketCap request failed: {exc}",
            "asset_count": 0,
            "top_movers_24h": [],
            "top_market_cap": [],
            "cached": False,
        }

    summary = summarize_coinmarketcap_listings(payload, mover_limit=mover_limit)
    cache_path.parent.mkdir(parents=True, exist_ok=True)
    cache_path.write_text(json.dumps({"fetched_at": now, "summary": summary}, indent=2) + "\n", encoding="utf-8")
    return summary
