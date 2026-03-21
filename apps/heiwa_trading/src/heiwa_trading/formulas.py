from __future__ import annotations

import math


def clamp_probability(value: float, epsilon: float = 1e-6) -> float:
    return min(1.0 - epsilon, max(epsilon, value))


def expected_value(probability: float, price: float) -> float:
    return probability - price


def kelly_fraction(probability: float, price: float) -> float:
    probability = clamp_probability(probability)
    price = clamp_probability(price)
    edge = expected_value(probability, price)
    if edge <= 0:
        return 0.0
    return max(0.0, edge / (1.0 - price))


def bayesian_update(prior: float, likelihood_if_true: float, likelihood_if_false: float) -> float:
    prior = clamp_probability(prior)
    likelihood_if_true = clamp_probability(likelihood_if_true)
    likelihood_if_false = clamp_probability(likelihood_if_false)
    numerator = likelihood_if_true * prior
    denominator = numerator + (likelihood_if_false * (1.0 - prior))
    return numerator / denominator


def log_odds(probability: float) -> float:
    probability = clamp_probability(probability)
    return math.log(probability / (1.0 - probability))


def log_odds_edge(subjective_probability: float, market_probability: float) -> float:
    return log_odds(subjective_probability) - log_odds(market_probability)
