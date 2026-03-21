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
