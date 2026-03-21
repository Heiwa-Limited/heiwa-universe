from __future__ import annotations

from heiwa_trading.formulas import bayesian_update, expected_value, kelly_fraction, log_odds_edge
from heiwa_trading.types import NormalizedMarket, RiskPolicy, ScoreDecision


def score_market(
    *,
    market: NormalizedMarket,
    subjective_probability: float,
    policy: RiskPolicy,
    evidence_likelihood_true: float | None = None,
    evidence_likelihood_false: float | None = None,
) -> ScoreDecision:
    if evidence_likelihood_true is not None and evidence_likelihood_false is not None:
        posterior_probability = bayesian_update(
            prior=subjective_probability,
            likelihood_if_true=evidence_likelihood_true,
            likelihood_if_false=evidence_likelihood_false,
        )
    else:
        posterior_probability = subjective_probability

    if not market.active or market.closed or not market.enable_order_book:
        return ScoreDecision(
            action="SKIP",
            market_probability=market.yes_price,
            subjective_probability=subjective_probability,
            posterior_probability=posterior_probability,
            expected_value=0.0,
            kelly_fraction=0.0,
            log_odds_edge=0.0,
            recommended_notional=0.0,
            rationale="Market is not tradeable in the current policy.",
        )

    edge = expected_value(posterior_probability, market.yes_price)
    kelly = min(policy.max_kelly_fraction, kelly_fraction(posterior_probability, market.yes_price))
    odds_gap = log_odds_edge(posterior_probability, market.yes_price)
    recommended_notional = round(
        min(policy.max_trade_notional, policy.max_total_capital * kelly),
        2,
    )

    if edge >= policy.min_expected_value and odds_gap >= policy.min_log_odds_edge and recommended_notional > 0:
        return ScoreDecision(
            action="BUY",
            market_probability=market.yes_price,
            subjective_probability=subjective_probability,
            posterior_probability=posterior_probability,
            expected_value=edge,
            kelly_fraction=kelly,
            log_odds_edge=odds_gap,
            recommended_notional=recommended_notional,
            rationale="Positive edge cleared EV/log-odds thresholds within cheap-risk policy.",
        )

    return ScoreDecision(
        action="SKIP",
        market_probability=market.yes_price,
        subjective_probability=subjective_probability,
        posterior_probability=posterior_probability,
        expected_value=edge,
        kelly_fraction=kelly,
        log_odds_edge=odds_gap,
        recommended_notional=0.0,
        rationale="Signal did not clear the cheap-risk thresholds.",
    )
