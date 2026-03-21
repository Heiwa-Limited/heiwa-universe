from __future__ import annotations

import json
from pathlib import Path
from statistics import mean
from typing import Iterable

from heiwa_trading.config import SCAN_HISTORY_PATH, TUNING_REPORT_PATH
from heiwa_trading.strategy import score_market
from heiwa_trading.tournament import TournamentState, _subjective_probability, leaderboard_rows, run_tournament_step
from heiwa_trading.types import NormalizedMarket


def _source_statuses(
    sources: Iterable[str],
    market_count: int,
    *,
    source_details: dict[str, dict[str, object]] | None = None,
) -> dict[str, dict[str, object]]:
    source_details = source_details or {}
    statuses: dict[str, dict[str, object]] = {}
    for source in sources:
        if source in source_details:
            detail = source_details[source]
            statuses[source] = {
                "status": detail.get("status", "ok"),
                "message": detail.get("message", ""),
            }
            if "asset_count" in detail:
                statuses[source]["asset_count"] = detail["asset_count"]
            if "cached" in detail:
                statuses[source]["cached"] = detail["cached"]
            continue
        if source == "polymarket":
            statuses[source] = {
                "status": "ok",
                "message": "Fetched public Polymarket market data.",
                "markets": market_count,
            }
        elif source == "coinmarketcap":
            statuses[source] = {
                "status": "pending_config",
                "message": "Waiting for CoinMarketCap API/WebSocket details.",
            }
        else:
            statuses[source] = {
                "status": "unsupported",
                "message": f"Unsupported source '{source}'.",
            }
    return statuses


def _top_opportunities(state: TournamentState, markets: list[NormalizedMarket], *, limit: int = 5) -> list[dict[str, object]]:
    opportunities: list[dict[str, object]] = []
    for market in markets:
        buy_decisions = []
        all_decisions = []
        for variant in state.variants:
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
            all_decisions.append(decision)
            if decision.action == "BUY":
                buy_decisions.append(decision)

        if not buy_decisions:
            continue

        opportunities.append(
            {
                "slug": market.slug,
                "question": market.question,
                "yes_price": market.yes_price,
                "buy_signals": len(buy_decisions),
                "avg_expected_value": round(mean(decision.expected_value for decision in buy_decisions), 4),
                "avg_log_odds_edge": round(mean(decision.log_odds_edge for decision in buy_decisions), 4),
                "avg_probability": round(mean(decision.posterior_probability for decision in all_decisions), 4),
                "liquidity": market.liquidity,
            }
        )

    return sorted(
        opportunities,
        key=lambda row: (row["buy_signals"], row["avg_expected_value"], row["avg_log_odds_edge"], row["liquidity"]),
        reverse=True,
    )[:limit]


def build_scan_summary(
    *,
    state: TournamentState,
    markets: list[NormalizedMarket],
    sources: Iterable[str],
    timestamp: str,
    source_details: dict[str, dict[str, object]] | None = None,
    top_limit: int = 5,
) -> tuple[TournamentState, dict[str, object]]:
    next_state = run_tournament_step(state=state, markets=markets, timestamp=timestamp)
    summary = {
        "timestamp": timestamp,
        "tick": next_state.tick,
        "market_count": len(markets),
        "source_statuses": _source_statuses(sources, len(markets), source_details=source_details),
        "source_snapshots": source_details or {},
        "top_opportunities": _top_opportunities(state, markets, limit=top_limit),
        "leaderboard": leaderboard_rows(next_state)[:top_limit],
    }
    return next_state, summary


def append_scan_history(summary: dict[str, object], path: Path = SCAN_HISTORY_PATH) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(summary) + "\n")


def load_scan_history(path: Path = SCAN_HISTORY_PATH, *, limit: int | None = None) -> list[dict[str, object]]:
    if not path.exists():
        return []
    with path.open("r", encoding="utf-8") as handle:
        rows = [json.loads(line) for line in handle if line.strip()]
    if limit is None:
        return rows
    return rows[-limit:]


def build_tuning_report(
    *,
    state: TournamentState,
    history: list[dict[str, object]],
    timestamp: str | None = None,
) -> dict[str, object]:
    latest_rows = leaderboard_rows(state)
    if history and all(row["equity"] == 0 and row["trade_count"] == 0 for row in latest_rows):
        latest_rows = history[-1]["leaderboard"]

    latest_by_wallet = {row["wallet_id"]: row for row in latest_rows}
    metrics: dict[str, dict[str, object]] = {
        variant.wallet_id: {
            "wallet_id": variant.wallet_id,
            "label": variant.label,
            "snapshots": 0,
            "rank_sum": 0.0,
            "top_finishes": 0,
            "best_equity": 0.0,
            "trade_count": 0,
            "latest_equity": 0.0,
        }
        for variant in state.variants
    }

    for snapshot in history:
        leaderboard = snapshot.get("leaderboard", [])
        for rank, row in enumerate(leaderboard, start=1):
            wallet_id = row["wallet_id"]
            metric = metrics.setdefault(
                wallet_id,
                {
                    "wallet_id": wallet_id,
                    "label": row.get("label", wallet_id),
                    "snapshots": 0,
                    "rank_sum": 0.0,
                    "top_finishes": 0,
                    "best_equity": 0.0,
                    "trade_count": 0,
                    "latest_equity": 0.0,
                },
            )
            metric["snapshots"] += 1
            metric["rank_sum"] += rank
            metric["top_finishes"] += 1 if rank == 1 else 0
            metric["best_equity"] = max(float(metric["best_equity"]), float(row.get("equity", 0.0)))
            metric["trade_count"] = max(int(metric["trade_count"]), int(row.get("trade_count", 0)))

    rankings = []
    for wallet_id, metric in metrics.items():
        latest_row = latest_by_wallet.get(wallet_id, {})
        latest_equity = float(latest_row.get("equity", metric["best_equity"]))
        trade_count = int(latest_row.get("trade_count", metric["trade_count"]))
        snapshots = int(metric["snapshots"])
        avg_rank = round(float(metric["rank_sum"]) / snapshots, 3) if snapshots else None
        top_finishes = int(metric["top_finishes"])
        score = round(
            latest_equity
            + (top_finishes * 0.5)
            + ((1.0 / avg_rank) if avg_rank else 0.0)
            + (trade_count * 0.01),
            4,
        )
        rankings.append(
            {
                "wallet_id": wallet_id,
                "label": metric["label"],
                "latest_equity": round(latest_equity, 4),
                "best_equity": round(float(metric["best_equity"]), 4),
                "trade_count": trade_count,
                "snapshots": snapshots,
                "avg_rank": avg_rank,
                "top_finishes": top_finishes,
                "score": score,
            }
        )

    rankings.sort(key=lambda row: (row["score"], row["latest_equity"], row["top_finishes"]), reverse=True)
    recommended = rankings[0] if rankings else None
    return {
        "generated_at": timestamp,
        "ticks_observed": len(history),
        "recommended_wallet_id": recommended["wallet_id"] if recommended else None,
        "recommended_label": recommended["label"] if recommended else None,
        "rankings": rankings,
    }


def save_tuning_report(report: dict[str, object], path: Path = TUNING_REPORT_PATH) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
