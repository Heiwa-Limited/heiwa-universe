from __future__ import annotations

import json
import time
from dataclasses import asdict, dataclass, replace
from pathlib import Path
from typing import Iterable

from heiwa_trading.config import CHEAP_POLICY, TOURNAMENT_STATE_PATH
from heiwa_trading.formulas import clamp_probability
from heiwa_trading.paper_trader import PaperPortfolio, PaperTrade, load_portfolio
from heiwa_trading.strategy import score_market
from heiwa_trading.types import NormalizedMarket, RiskPolicy


@dataclass(frozen=True)
class StrategyVariant:
    wallet_id: str
    label: str
    probability_bias: float
    momentum_weight: float
    reversion_weight: float
    liquidity_weight: float
    min_expected_value: float
    min_log_odds_edge: float
    max_kelly_fraction: float

    def policy(self, base_policy: RiskPolicy) -> RiskPolicy:
        return RiskPolicy(
            max_daily_loss=base_policy.max_daily_loss,
            max_trade_notional=base_policy.max_trade_notional,
            max_total_capital=base_policy.max_total_capital,
            min_expected_value=self.min_expected_value,
            min_log_odds_edge=self.min_log_odds_edge,
            max_kelly_fraction=self.max_kelly_fraction,
        )


@dataclass(frozen=True)
class WalletState:
    wallet_id: str
    label: str
    portfolio: PaperPortfolio
    unrealized_pnl: float = 0.0
    trade_count: int = 0
    last_action: str = "IDLE"


@dataclass(frozen=True)
class TournamentState:
    policy: RiskPolicy
    variants: tuple[StrategyVariant, ...]
    wallets: tuple[WalletState, ...]
    tick: int
    last_prices: dict[str, float]

    @classmethod
    def empty(cls, *, policy: RiskPolicy, variants: Iterable[StrategyVariant]) -> "TournamentState":
        variants_tuple = tuple(variants)
        wallets = tuple(
            WalletState(
                wallet_id=variant.wallet_id,
                label=variant.label,
                portfolio=PaperPortfolio.empty(policy=policy),
            )
            for variant in variants_tuple
        )
        return cls(policy=policy, variants=variants_tuple, wallets=wallets, tick=0, last_prices={})

    def to_dict(self) -> dict[str, object]:
        return {
            "policy": asdict(self.policy),
            "variants": [asdict(variant) for variant in self.variants],
            "wallets": [
                {
                    "wallet_id": wallet.wallet_id,
                    "label": wallet.label,
                    "portfolio": wallet.portfolio.to_dict(),
                    "unrealized_pnl": wallet.unrealized_pnl,
                    "trade_count": wallet.trade_count,
                    "last_action": wallet.last_action,
                }
                for wallet in self.wallets
            ],
            "tick": self.tick,
            "last_prices": self.last_prices,
        }

    @classmethod
    def from_dict(cls, payload: dict[str, object]) -> "TournamentState":
        policy = RiskPolicy(**payload["policy"])
        variants = tuple(StrategyVariant(**item) for item in payload["variants"])
        wallets = tuple(
            WalletState(
                wallet_id=item["wallet_id"],
                label=item["label"],
                portfolio=PaperPortfolio.from_dict(item["portfolio"]),
                unrealized_pnl=float(item.get("unrealized_pnl", 0.0)),
                trade_count=int(item.get("trade_count", 0)),
                last_action=str(item.get("last_action", "IDLE")),
            )
            for item in payload["wallets"]
        )
        return cls(
            policy=policy,
            variants=variants,
            wallets=wallets,
            tick=int(payload.get("tick", 0)),
            last_prices={str(k): float(v) for k, v in payload.get("last_prices", {}).items()},
        )


def build_default_variants() -> tuple[StrategyVariant, ...]:
    return (
        StrategyVariant("wallet-01", "steady-bias", 0.015, 0.20, 0.05, 0.02, 0.020, 0.050, 0.10),
        StrategyVariant("wallet-02", "steady-tight", 0.010, 0.15, 0.08, 0.02, 0.025, 0.060, 0.08),
        StrategyVariant("wallet-03", "momentum-lite", 0.008, 0.35, 0.04, 0.01, 0.020, 0.050, 0.10),
        StrategyVariant("wallet-04", "momentum-hard", 0.005, 0.50, 0.02, 0.01, 0.018, 0.045, 0.10),
        StrategyVariant("wallet-05", "reversion-lite", 0.012, 0.10, 0.12, 0.01, 0.020, 0.055, 0.09),
        StrategyVariant("wallet-06", "reversion-hard", 0.015, 0.05, 0.18, 0.01, 0.022, 0.060, 0.08),
        StrategyVariant("wallet-07", "liquidity-tilt", 0.010, 0.18, 0.06, 0.05, 0.020, 0.050, 0.10),
        StrategyVariant("wallet-08", "aggressive-cap", 0.018, 0.28, 0.05, 0.02, 0.018, 0.045, 0.10),
        StrategyVariant("wallet-09", "defensive-cap", 0.010, 0.12, 0.08, 0.02, 0.028, 0.070, 0.06),
        StrategyVariant("wallet-10", "balanced-center", 0.012, 0.22, 0.07, 0.02, 0.020, 0.050, 0.09),
    )


def _liquidity_signal(liquidity: float) -> float:
    return min(liquidity / 10_000.0, 1.0) - 0.5


def _subjective_probability(
    *,
    market: NormalizedMarket,
    last_price: float | None,
    variant: StrategyVariant,
) -> float:
    momentum = 0.0 if last_price is None else (market.yes_price - last_price)
    distance_from_center = market.yes_price - 0.5
    estimate = (
        market.yes_price
        + variant.probability_bias
        + (variant.momentum_weight * momentum)
        - (variant.reversion_weight * distance_from_center)
        + (variant.liquidity_weight * _liquidity_signal(market.liquidity))
    )
    return clamp_probability(estimate)


def _mark_to_market(portfolio: PaperPortfolio, market_map: dict[str, NormalizedMarket]) -> float:
    pnl = 0.0
    for trade in portfolio.trades:
        if trade.status != "OPEN":
            continue
        market = market_map.get(trade.slug)
        if market is None or trade.price <= 0:
            continue
        current_price = market.yes_price if trade.side == "YES" else market.no_price
        pnl += trade.notional * ((current_price / trade.price) - 1.0)
    return round(pnl, 4)


def _should_close_trade(trade: PaperTrade, market: NormalizedMarket) -> bool:
    current_price = market.yes_price if trade.side == "YES" else market.no_price
    take_profit_hit = current_price >= trade.price * 1.12
    stop_loss_hit = current_price <= trade.price * 0.92
    return market.closed or not market.active or take_profit_hit or stop_loss_hit


def _iter_wallet_variants(state: TournamentState):
    if len(state.wallets) != len(state.variants):
        raise ValueError("Tournament state is inconsistent: wallet and variant counts differ")
    return zip(state.wallets, state.variants)


def run_tournament_step(
    *,
    state: TournamentState,
    markets: list[NormalizedMarket],
    timestamp: str | None = None,
    max_trades_per_wallet: int | None = None,
) -> TournamentState:
    timestamp = timestamp or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    next_wallets: list[WalletState] = []
    market_map = {market.slug: market for market in markets}

    for wallet, variant in _iter_wallet_variants(state):
        portfolio = wallet.portfolio
        trade_count = wallet.trade_count
        last_action = "HOLD"
        for trade in tuple(portfolio.trades):
            if trade.status != "OPEN":
                continue
            market = market_map.get(trade.slug)
            if market is None or not _should_close_trade(trade, market):
                continue
            price = market.yes_price if trade.side == "YES" else market.no_price
            portfolio = portfolio.close_positions(
                slug=trade.slug,
                side=trade.side,
                price=price,
                timestamp=timestamp,
            )
            last_action = f"SELL:{trade.slug}"
        for market in markets:
            if max_trades_per_wallet is not None and trade_count >= max_trades_per_wallet:
                break
            if portfolio.has_position(market.slug, side="YES"):
                continue
            subjective_probability = _subjective_probability(
                market=market,
                last_price=state.last_prices.get(market.slug),
                variant=variant,
            )
            decision = score_market(
                market=market,
                subjective_probability=subjective_probability,
                policy=variant.policy(state.policy),
            )
            if decision.action != "BUY":
                continue
            trade = PaperTrade(
                trade_id=f"{wallet.wallet_id}-{state.tick + 1}-{market.slug}",
                slug=market.slug,
                side="YES",
                price=market.yes_price,
                probability=decision.posterior_probability,
                notional=decision.recommended_notional,
                timestamp=timestamp,
            )
            try:
                portfolio = portfolio.apply_trade(trade)
                trade_count += 1
                last_action = f"BUY:{market.slug}"
            except ValueError:
                last_action = "RISK_LIMIT"
                break
        next_wallets.append(
            WalletState(
                wallet_id=wallet.wallet_id,
                label=wallet.label,
                portfolio=portfolio,
                unrealized_pnl=_mark_to_market(portfolio, market_map),
                trade_count=trade_count,
                last_action=last_action,
            )
        )

    return TournamentState(
        policy=state.policy,
        variants=state.variants,
        wallets=tuple(next_wallets),
        tick=state.tick + 1,
        last_prices={market.slug: market.yes_price for market in markets},
    )


def leaderboard_rows(state: TournamentState) -> list[dict[str, object]]:
    rows = [
        {
            "wallet_id": wallet.wallet_id,
            "label": wallet.label,
            "trade_count": wallet.trade_count,
            "open_notional": wallet.portfolio.open_notional,
            "unrealized_pnl": wallet.unrealized_pnl,
            "equity": round(wallet.portfolio.open_notional + wallet.unrealized_pnl, 4),
            "last_action": wallet.last_action,
        }
        for wallet in state.wallets
    ]
    return sorted(rows, key=lambda row: (row["equity"], row["unrealized_pnl"], row["open_notional"]), reverse=True)


def load_tournament_state(path: Path = TOURNAMENT_STATE_PATH) -> TournamentState:
    if not path.exists():
        return TournamentState.empty(policy=CHEAP_POLICY, variants=build_default_variants())
    return TournamentState.from_dict(json.loads(path.read_text()))


def save_tournament_state(state: TournamentState, path: Path = TOURNAMENT_STATE_PATH) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state.to_dict(), indent=2) + "\n")
