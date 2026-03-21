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
