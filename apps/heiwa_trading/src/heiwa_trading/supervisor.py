from __future__ import annotations

import json
from dataclasses import asdict, dataclass, replace
from datetime import datetime
from pathlib import Path
from uuid import uuid4

from heiwa_trading.config import CHEAP_POLICY, SUPERVISOR_STATE_PATH
from heiwa_trading.paper_trader import PaperPortfolio
from heiwa_trading.scan import _top_opportunities
from heiwa_trading.tournament import StrategyVariant, TournamentState, WalletState, build_default_variants, leaderboard_rows, run_tournament_step
from heiwa_trading.types import NormalizedMarket, RiskPolicy


DEFAULT_HOLD_WINDOWS = (300, 900, 3600, 14400)
DEFAULT_TARGET_FILLS = 100


def _parse_timestamp(value: str) -> datetime:
    return datetime.fromisoformat(value)


def _window_label(seconds: int) -> str:
    if seconds < 3600:
        return f"{seconds // 60}m"
    return f"{seconds // 3600}h"


@dataclass(frozen=True)
class CohortState:
    cohort_id: str
    tournament: TournamentState
    target_fills: int
    hold_windows: tuple[int, ...]
    created_at: str
    status: str = "active"
    fills_completed_at: str | None = None
    scored_windows: dict[str, list[dict[str, object]]] | None = None
    lead_duration_seconds: dict[str, float] | None = None
    current_leader_wallet_id: str | None = None
    last_leader_observed_at: str | None = None
    winner_wallet_id: str | None = None
    winner_label: str | None = None
    winner_score: float | None = None
    promotion_applied: bool = False

    def to_dict(self) -> dict[str, object]:
        return {
            "cohort_id": self.cohort_id,
            "tournament": self.tournament.to_dict(),
            "target_fills": self.target_fills,
            "hold_windows": list(self.hold_windows),
            "created_at": self.created_at,
            "status": self.status,
            "fills_completed_at": self.fills_completed_at,
            "scored_windows": self.scored_windows or {},
            "lead_duration_seconds": self.lead_duration_seconds or {},
            "current_leader_wallet_id": self.current_leader_wallet_id,
            "last_leader_observed_at": self.last_leader_observed_at,
            "winner_wallet_id": self.winner_wallet_id,
            "winner_label": self.winner_label,
            "winner_score": self.winner_score,
            "promotion_applied": self.promotion_applied,
        }

    @classmethod
    def from_dict(cls, payload: dict[str, object]) -> "CohortState":
        return cls(
            cohort_id=str(payload["cohort_id"]),
            tournament=TournamentState.from_dict(payload["tournament"]),
            target_fills=int(payload["target_fills"]),
            hold_windows=tuple(int(item) for item in payload["hold_windows"]),
            created_at=str(payload["created_at"]),
            status=str(payload.get("status", "active")),
            fills_completed_at=payload.get("fills_completed_at"),
            scored_windows={str(k): v for k, v in payload.get("scored_windows", {}).items()},
            lead_duration_seconds={str(k): float(v) for k, v in payload.get("lead_duration_seconds", {}).items()},
            current_leader_wallet_id=payload.get("current_leader_wallet_id"),
            last_leader_observed_at=payload.get("last_leader_observed_at"),
            winner_wallet_id=payload.get("winner_wallet_id"),
            winner_label=payload.get("winner_label"),
            winner_score=float(payload["winner_score"]) if payload.get("winner_score") is not None else None,
            promotion_applied=bool(payload.get("promotion_applied", False)),
        )


@dataclass(frozen=True)
class LiveBotState:
    wallet_id: str
    label: str
    variant: StrategyVariant
    source_cohort_id: str
    promotion_score: float
    portfolio: dict[str, object]
    unrealized_pnl: float
    trade_count: int
    last_action: str

    def to_dict(self) -> dict[str, object]:
        return {
            "wallet_id": self.wallet_id,
            "label": self.label,
            "variant": asdict(self.variant),
            "source_cohort_id": self.source_cohort_id,
            "promotion_score": self.promotion_score,
            "portfolio": self.portfolio,
            "unrealized_pnl": self.unrealized_pnl,
            "trade_count": self.trade_count,
            "last_action": self.last_action,
        }

    @classmethod
    def from_wallet(cls, wallet: WalletState, variant: StrategyVariant, *, source_cohort_id: str, promotion_score: float) -> "LiveBotState":
        return cls(
            wallet_id=wallet.wallet_id,
            label=wallet.label,
            variant=variant,
            source_cohort_id=source_cohort_id,
            promotion_score=promotion_score,
            portfolio=wallet.portfolio.to_dict(),
            unrealized_pnl=wallet.unrealized_pnl,
            trade_count=wallet.trade_count,
            last_action=wallet.last_action,
        )

    @classmethod
    def from_dict(cls, payload: dict[str, object]) -> "LiveBotState":
        return cls(
            wallet_id=str(payload["wallet_id"]),
            label=str(payload["label"]),
            variant=StrategyVariant(**payload["variant"]),
            source_cohort_id=str(payload["source_cohort_id"]),
            promotion_score=float(payload["promotion_score"]),
            portfolio=dict(payload["portfolio"]),
            unrealized_pnl=float(payload.get("unrealized_pnl", 0.0)),
            trade_count=int(payload.get("trade_count", 0)),
            last_action=str(payload.get("last_action", "IDLE")),
        )


@dataclass(frozen=True)
class SupervisorState:
    policy: RiskPolicy
    active_cohort: CohortState | None
    archived_cohorts: tuple[CohortState, ...]
    live_top5: tuple[LiveBotState, ...]
    source_statuses: dict[str, dict[str, object]]
    source_snapshots: dict[str, dict[str, object]]
    last_scan_at: str | None = None
    last_top_opportunities: tuple[dict[str, object], ...] = ()
    live_last_prices: dict[str, float] | None = None

    def to_dict(self) -> dict[str, object]:
        return {
            "policy": asdict(self.policy),
            "active_cohort": self.active_cohort.to_dict() if self.active_cohort else None,
            "archived_cohorts": [item.to_dict() for item in self.archived_cohorts],
            "live_top5": [item.to_dict() for item in self.live_top5],
            "source_statuses": self.source_statuses,
            "source_snapshots": self.source_snapshots,
            "last_scan_at": self.last_scan_at,
            "last_top_opportunities": list(self.last_top_opportunities),
            "live_last_prices": self.live_last_prices or {},
        }

    @classmethod
    def from_dict(cls, payload: dict[str, object]) -> "SupervisorState":
        return cls(
            policy=RiskPolicy(**payload["policy"]),
            active_cohort=CohortState.from_dict(payload["active_cohort"]) if payload.get("active_cohort") else None,
            archived_cohorts=tuple(CohortState.from_dict(item) for item in payload.get("archived_cohorts", [])),
            live_top5=tuple(LiveBotState.from_dict(item) for item in payload.get("live_top5", [])),
            source_statuses={str(k): v for k, v in payload.get("source_statuses", {}).items()},
            source_snapshots={str(k): v for k, v in payload.get("source_snapshots", {}).items()},
            last_scan_at=payload.get("last_scan_at"),
            last_top_opportunities=tuple(payload.get("last_top_opportunities", [])),
            live_last_prices={str(k): float(v) for k, v in payload.get("live_last_prices", {}).items()},
        )


def create_supervisor_state(*, policy: RiskPolicy = CHEAP_POLICY) -> SupervisorState:
    return SupervisorState(
        policy=policy,
        active_cohort=None,
        archived_cohorts=(),
        live_top5=(),
        source_statuses={},
        source_snapshots={},
        live_last_prices={},
    )


def _create_cohort(*, policy: RiskPolicy, target_fills: int, hold_windows: tuple[int, ...], timestamp: str) -> CohortState:
    return CohortState(
        cohort_id=f"cohort-{uuid4().hex[:8]}",
        tournament=TournamentState.empty(policy=policy, variants=build_default_variants()),
        target_fills=target_fills,
        hold_windows=hold_windows,
        created_at=timestamp,
        scored_windows={},
        lead_duration_seconds={},
    )


def _update_leader_duration(cohort: CohortState, *, timestamp: str) -> CohortState:
    rows = leaderboard_rows(cohort.tournament)
    if not rows:
        return cohort
    current_leader = str(rows[0]["wallet_id"])
    durations = dict(cohort.lead_duration_seconds or {})
    if cohort.current_leader_wallet_id and cohort.last_leader_observed_at:
        delta = (_parse_timestamp(timestamp) - _parse_timestamp(cohort.last_leader_observed_at)).total_seconds()
        durations[cohort.current_leader_wallet_id] = durations.get(cohort.current_leader_wallet_id, 0.0) + max(delta, 0.0)
    return replace(
        cohort,
        lead_duration_seconds=durations,
        current_leader_wallet_id=current_leader,
        last_leader_observed_at=timestamp,
    )


def _score_due_windows(cohort: CohortState, *, timestamp: str) -> CohortState:
    if cohort.fills_completed_at is None:
        return cohort
    elapsed = (_parse_timestamp(timestamp) - _parse_timestamp(cohort.fills_completed_at)).total_seconds()
    scored = dict(cohort.scored_windows or {})
    updated = _update_leader_duration(cohort, timestamp=timestamp)
    for window in cohort.hold_windows:
        label = _window_label(window)
        if label in scored or elapsed < window:
            continue
        scored[label] = leaderboard_rows(updated.tournament)
    winner_wallet_id = updated.winner_wallet_id
    winner_label = updated.winner_label
    winner_score = updated.winner_score
    status = "cooldown"
    if scored and len(scored) == len(updated.hold_windows):
        final_rows = scored[_window_label(max(updated.hold_windows))]
        top_equity = final_rows[0]["equity"]
        tied = [row for row in final_rows if row["equity"] == top_equity]
        if len(tied) == 1:
            winner_row = tied[0]
        else:
            durations = updated.lead_duration_seconds or {}
            winner_row = max(tied, key=lambda row: durations.get(str(row["wallet_id"]), 0.0))
        winner_wallet_id = str(winner_row["wallet_id"])
        winner_label = str(winner_row["label"])
        winner_score = float(winner_row["equity"])
        status = "completed"
    return replace(
        updated,
        status=status,
        scored_windows=scored,
        winner_wallet_id=winner_wallet_id,
        winner_label=winner_label,
        winner_score=winner_score,
    )


def _tick_cohort(cohort: CohortState, *, markets: list[NormalizedMarket], timestamp: str) -> CohortState:
    if cohort.status == "active":
        tournament = run_tournament_step(
            state=cohort.tournament,
            markets=markets,
            timestamp=timestamp,
            max_trades_per_wallet=cohort.target_fills,
        )
        fills_completed_at = cohort.fills_completed_at
        status = cohort.status
        if fills_completed_at is None and all(wallet.trade_count >= cohort.target_fills for wallet in tournament.wallets):
            fills_completed_at = timestamp
            status = "cooldown"
        cohort = replace(
            cohort,
            tournament=tournament,
            fills_completed_at=fills_completed_at,
            status=status,
        )
    return _score_due_windows(cohort, timestamp=timestamp)


def _promote_winner(cohort: CohortState, live_top5: tuple[LiveBotState, ...]) -> tuple[tuple[LiveBotState, ...], bool]:
    if cohort.winner_wallet_id is None or cohort.winner_score is None:
        return live_top5, False
    variant_map = {variant.wallet_id: variant for variant in cohort.tournament.variants}
    wallet_map = {wallet.wallet_id: wallet for wallet in cohort.tournament.wallets}
    winner_id = cohort.winner_wallet_id
    candidate = LiveBotState.from_wallet(
        wallet_map[winner_id],
        variant_map[winner_id],
        source_cohort_id=cohort.cohort_id,
        promotion_score=cohort.winner_score,
    )
    merged = [item for item in live_top5 if item.wallet_id != candidate.wallet_id]
    merged.append(candidate)
    merged.sort(key=lambda item: (item.promotion_score, item.unrealized_pnl, item.trade_count), reverse=True)
    trimmed = tuple(merged[:5])
    return trimmed, any(item.wallet_id == candidate.wallet_id for item in trimmed)


def _tick_live_top5(
    live_top5: tuple[LiveBotState, ...],
    *,
    policy: RiskPolicy,
    markets: list[NormalizedMarket],
    timestamp: str,
    last_prices: dict[str, float],
) -> tuple[tuple[LiveBotState, ...], dict[str, float]]:
    if not live_top5:
        return live_top5, last_prices
    variants = tuple(item.variant for item in live_top5)
    if len(live_top5) != len(variants):
        raise ValueError("Live top-5 state is inconsistent: bot and variant counts differ")
    rebuilt_wallets = tuple(
        WalletState(
            wallet_id=item.wallet_id,
            label=item.label,
            portfolio=PaperPortfolio.from_dict(item.portfolio),
            unrealized_pnl=item.unrealized_pnl,
            trade_count=item.trade_count,
            last_action=item.last_action,
        )
        for item in live_top5
    )
    temp_state = TournamentState(
        policy=policy,
        variants=variants,
        wallets=rebuilt_wallets,
        tick=0,
        last_prices=last_prices,
    )
    next_state = run_tournament_step(state=temp_state, markets=markets, timestamp=timestamp)
    updated = []
    if len(next_state.wallets) != len(live_top5):
        raise ValueError("Live top-5 rebuild produced a mismatched wallet count")
    for item, wallet, variant in zip(live_top5, next_state.wallets, variants):
        updated.append(
            LiveBotState.from_wallet(
                wallet,
                variant,
                source_cohort_id=item.source_cohort_id,
                promotion_score=item.promotion_score,
            )
        )
    updated.sort(key=lambda item: (item.promotion_score, item.unrealized_pnl, item.trade_count), reverse=True)
    return tuple(updated), next_state.last_prices


def leaderboard_top5(state: SupervisorState) -> list[dict[str, object]]:
    rows = [
        {
            "wallet_id": item.wallet_id,
            "label": item.label,
            "promotion_score": item.promotion_score,
            "equity": round(float(item.portfolio.get("open_notional", 0.0)) + item.unrealized_pnl, 4),
            "trade_count": item.trade_count,
            "last_action": item.last_action,
            "source_cohort_id": item.source_cohort_id,
        }
        for item in state.live_top5
    ]
    return sorted(rows, key=lambda row: (row["promotion_score"], row["equity"], row["trade_count"]), reverse=True)


def _cohort_for_summary(state: SupervisorState) -> CohortState | None:
    if state.active_cohort is not None:
        return state.active_cohort
    if state.archived_cohorts:
        return state.archived_cohorts[-1]
    return None


def build_supervisor_summary(state: SupervisorState) -> dict[str, object]:
    cohort = _cohort_for_summary(state)
    cohort_summary = None
    if cohort is not None:
        fills_progress = {wallet.wallet_id: wallet.trade_count for wallet in cohort.tournament.wallets}
        average_fill_ratio = 0.0
        if cohort.target_fills > 0 and fills_progress:
            average_fill_ratio = sum(min(count / cohort.target_fills, 1.0) for count in fills_progress.values()) / len(fills_progress)
        cohort_summary = {
            "cohort_id": cohort.cohort_id,
            "status": cohort.status,
            "target_fills": cohort.target_fills,
            "fills_progress": fills_progress,
            "progress_percent": round(average_fill_ratio * 100.0, 2),
            "fills_completed_at": cohort.fills_completed_at,
            "scored_windows": cohort.scored_windows or {},
            "leaderboard": leaderboard_rows(cohort.tournament),
            "archived_winners": [
                {
                    "cohort_id": item.cohort_id,
                    "wallet_id": item.winner_wallet_id,
                    "label": item.winner_label,
                    "score": item.winner_score,
                }
                for item in reversed(state.archived_cohorts)
                if item.winner_wallet_id is not None
            ][:5],
            "winner": (
                {
                    "wallet_id": cohort.winner_wallet_id,
                    "label": cohort.winner_label,
                    "score": cohort.winner_score,
                }
                if cohort.winner_wallet_id
                else None
            ),
        }
    return {
        "timestamp": state.last_scan_at,
        "sources": state.source_statuses,
        "source_snapshots": state.source_snapshots,
        "cohort": cohort_summary,
        "live_top5": leaderboard_top5(state),
        "top_opportunities": list(state.last_top_opportunities),
    }


def supervisor_tick(
    *,
    state: SupervisorState,
    markets: list[NormalizedMarket],
    timestamp: str,
    target_fills: int = DEFAULT_TARGET_FILLS,
    hold_windows: tuple[int, ...] = DEFAULT_HOLD_WINDOWS,
    source_statuses: dict[str, dict[str, object]] | None = None,
    source_snapshots: dict[str, dict[str, object]] | None = None,
) -> tuple[SupervisorState, dict[str, object]]:
    active_cohort = state.active_cohort or _create_cohort(
        policy=state.policy,
        target_fills=target_fills,
        hold_windows=hold_windows,
        timestamp=timestamp,
    )
    active_cohort = _tick_cohort(active_cohort, markets=markets, timestamp=timestamp)

    live_top5 = state.live_top5
    archived = list(state.archived_cohorts)
    summary_cohort = active_cohort
    if active_cohort.status == "completed" and not active_cohort.promotion_applied:
        live_top5, promoted = _promote_winner(active_cohort, live_top5)
        active_cohort = replace(active_cohort, promotion_applied=promoted)
    if active_cohort.status == "completed":
        archived.append(active_cohort)
        next_active = None
    else:
        next_active = active_cohort

    live_top5, live_last_prices = _tick_live_top5(
        live_top5,
        policy=state.policy,
        markets=markets,
        timestamp=timestamp,
        last_prices=state.live_last_prices or {},
    )
    opportunity_state = summary_cohort.tournament if summary_cohort else TournamentState.empty(policy=state.policy, variants=build_default_variants())
    top_opportunities = _top_opportunities(opportunity_state, markets, limit=5)
    next_state = SupervisorState(
        policy=state.policy,
        active_cohort=next_active,
        archived_cohorts=tuple(archived),
        live_top5=live_top5,
        source_statuses=source_statuses or {},
        source_snapshots=source_snapshots or {},
        last_scan_at=timestamp,
        last_top_opportunities=tuple(top_opportunities),
        live_last_prices=live_last_prices,
    )
    return next_state, build_supervisor_summary(next_state if next_active is not None else replace(next_state, active_cohort=None))


def load_supervisor_state(path: Path = SUPERVISOR_STATE_PATH) -> SupervisorState:
    if not path.exists():
        return create_supervisor_state()
    return SupervisorState.from_dict(json.loads(path.read_text()))


def save_supervisor_state(state: SupervisorState, path: Path = SUPERVISOR_STATE_PATH) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state.to_dict(), indent=2) + "\n")
