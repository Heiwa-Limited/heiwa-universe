from __future__ import annotations

from dataclasses import asdict, dataclass


@dataclass(frozen=True)
class RiskPolicy:
    max_daily_loss: float
    max_trade_notional: float
    max_total_capital: float
    min_expected_value: float = 0.02
    min_log_odds_edge: float = 0.05
    max_kelly_fraction: float = 0.10


@dataclass(frozen=True)
class NormalizedMarket:
    market_id: str
    slug: str
    question: str
    yes_price: float
    no_price: float
    liquidity: float
    volume_24hr: float
    active: bool
    closed: bool
    enable_order_book: bool

    def to_dict(self) -> dict[str, object]:
        return asdict(self)


@dataclass(frozen=True)
class ScoreDecision:
    action: str
    market_probability: float
    subjective_probability: float
    posterior_probability: float
    expected_value: float
    kelly_fraction: float
    log_odds_edge: float
    recommended_notional: float
    rationale: str

    def to_dict(self) -> dict[str, object]:
        return asdict(self)
