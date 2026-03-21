from __future__ import annotations

import json
from typing import Any
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from heiwa_trading.config import MARKETS_BASE_URL, USER_AGENT
from heiwa_trading.types import NormalizedMarket


def _coerce_float(value: Any, default: float = 0.0) -> float:
    if value in (None, ""):
        return default
    return float(value)


def _parse_outcome_prices(raw_value: Any) -> list[float]:
    if isinstance(raw_value, str):
        decoded = json.loads(raw_value)
    else:
        decoded = raw_value
    values = [float(item) for item in decoded]
    if len(values) < 2:
        raise ValueError("Expected at least two outcome prices.")
    return values


def normalize_market(raw_market: dict[str, Any]) -> NormalizedMarket:
    yes_price, no_price = _parse_outcome_prices(raw_market["outcomePrices"])
    return NormalizedMarket(
        market_id=str(raw_market.get("id", "")),
        slug=str(raw_market.get("slug", "")),
        question=str(raw_market.get("question", "")),
        yes_price=yes_price,
        no_price=no_price,
        liquidity=_coerce_float(raw_market.get("liquidityNum", raw_market.get("liquidity"))),
        volume_24hr=_coerce_float(raw_market.get("volume24hr", raw_market.get("volume24hrClob", 0.0))),
        active=bool(raw_market.get("active", False)),
        closed=bool(raw_market.get("closed", False)),
        enable_order_book=bool(raw_market.get("enableOrderBook", False)),
    )


def fetch_markets(*, limit: int = 10, active: bool = True, closed: bool = False, slug: str | None = None) -> list[NormalizedMarket]:
    params = {
        "limit": limit,
        "active": str(active).lower(),
        "closed": str(closed).lower(),
    }
    if slug:
        params["slug"] = slug
    url = f"{MARKETS_BASE_URL}?{urlencode(params)}"
    request = Request(url, headers={"User-Agent": USER_AGENT})
    with urlopen(request, timeout=10) as response:
        payload = json.loads(response.read().decode("utf-8"))
    return [normalize_market(item) for item in payload]


def get_market_by_slug(slug: str) -> NormalizedMarket:
    markets = fetch_markets(limit=1, active=True, closed=False, slug=slug)
    if not markets:
        raise ValueError(f"No active market found for slug '{slug}'.")
    return markets[0]
