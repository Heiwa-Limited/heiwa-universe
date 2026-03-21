from __future__ import annotations

import json
from dataclasses import asdict, dataclass, replace
from pathlib import Path

from heiwa_trading.config import PORTFOLIO_PATH
from heiwa_trading.types import RiskPolicy


@dataclass(frozen=True)
class PaperTrade:
    trade_id: str
    slug: str
    side: str
    price: float
    probability: float
    notional: float
    timestamp: str
    status: str = "OPEN"
    exit_price: float | None = None
    exit_timestamp: str | None = None


@dataclass(frozen=True)
class PaperPortfolio:
    policy: RiskPolicy
    trades: tuple[PaperTrade, ...]
    open_notional: float
    realized_pnl: float = 0.0

    @classmethod
    def empty(cls, *, policy: RiskPolicy) -> "PaperPortfolio":
        return cls(policy=policy, trades=(), open_notional=0.0, realized_pnl=0.0)

    def apply_trade(self, trade: PaperTrade) -> "PaperPortfolio":
        if trade.notional > self.policy.max_trade_notional:
            raise ValueError("Trade exceeds max per-trade notional.")
        if self.open_notional + trade.notional > self.policy.max_total_capital:
            raise ValueError("Trade exceeds max total capital.")
        if abs(min(self.realized_pnl, 0.0)) >= self.policy.max_daily_loss:
            raise ValueError("Daily loss limit already reached.")
        return replace(
            self,
            trades=(*self.trades, trade),
            open_notional=round(self.open_notional + trade.notional, 2),
        )

    def close_positions(self, *, slug: str, side: str, price: float, timestamp: str) -> "PaperPortfolio":
        updated_trades: list[PaperTrade] = []
        realized_delta = 0.0
        freed_notional = 0.0
        for trade in self.trades:
            if trade.slug == slug and trade.side == side and trade.status == "OPEN":
                updated_trades.append(
                    replace(
                        trade,
                        status="CLOSED",
                        exit_price=price,
                        exit_timestamp=timestamp,
                    )
                )
                realized_delta += trade.notional * ((price / trade.price) - 1.0)
                freed_notional += trade.notional
                continue
            updated_trades.append(trade)
        return replace(
            self,
            trades=tuple(updated_trades),
            open_notional=round(max(0.0, self.open_notional - freed_notional), 2),
            realized_pnl=round(self.realized_pnl + realized_delta, 4),
        )

    def has_position(self, slug: str, side: str = "YES") -> bool:
        return any(trade.slug == slug and trade.side == side and trade.status == "OPEN" for trade in self.trades)

    def to_dict(self) -> dict[str, object]:
        return {
            "policy": asdict(self.policy),
            "trades": [asdict(trade) for trade in self.trades],
            "open_notional": self.open_notional,
            "realized_pnl": self.realized_pnl,
        }

    @classmethod
    def from_dict(cls, payload: dict[str, object]) -> "PaperPortfolio":
        policy = RiskPolicy(**payload["policy"])
        trades = tuple(PaperTrade(**item) for item in payload.get("trades", []))
        return cls(
            policy=policy,
            trades=trades,
            open_notional=float(payload.get("open_notional", 0.0)),
            realized_pnl=float(payload.get("realized_pnl", 0.0)),
        )


def load_portfolio(path: Path = PORTFOLIO_PATH, *, policy: RiskPolicy) -> PaperPortfolio:
    if not path.exists():
        return PaperPortfolio.empty(policy=policy)
    return PaperPortfolio.from_dict(json.loads(path.read_text()))


def save_portfolio(portfolio: PaperPortfolio, path: Path = PORTFOLIO_PATH) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(portfolio.to_dict(), indent=2) + "\n")
