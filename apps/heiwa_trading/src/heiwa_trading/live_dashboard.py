from __future__ import annotations


def build_live_dashboard_text(*, summary: dict[str, object], report: dict[str, object]) -> str:
    lines = []
    lines.append("mac-agent trading tournament monitor")
    lines.append(f"Tick {summary.get('tick')}")
    lines.append(
        "Recommended wallet: "
        f"{report.get('recommended_wallet_id')} ({report.get('recommended_label')})"
    )
    statuses = summary.get("source_statuses", {})
    lines.append(
        "Sources: "
        + ", ".join(f"{name}={payload.get('status')}" for name, payload in statuses.items())
    )
    lines.append("")
    lines.append("Top opportunities")
    for row in summary.get("top_opportunities", [])[:5]:
        lines.append(
            f"- {row.get('slug')} | buys={row.get('buy_signals')} | ev={row.get('avg_expected_value')}"
        )
    lines.append("")
    lines.append("Leaderboard")
    for row in summary.get("leaderboard", [])[:5]:
        lines.append(
            f"- {row.get('wallet_id')} {row.get('label')} | equity={row.get('equity')} | last={row.get('last_action')}"
        )
    coinmarketcap = summary.get("source_snapshots", {}).get("coinmarketcap", {})
    movers = coinmarketcap.get("top_movers_24h", [])
    if movers:
        lines.append("")
        lines.append("CoinMarketCap movers")
        for row in movers[:5]:
            lines.append(f"- {row.get('symbol')} | 24h={row.get('percent_change_24h')}%")
    return "\n".join(lines)


def build_supervisor_dashboard_text(summary: dict[str, object]) -> str:
    lines = []
    lines.append("mac-agent cohort supervisor")
    lines.append(f"As of {summary.get('timestamp')}")
    lines.append(
        "Sources: "
        + ", ".join(f"{name}={payload.get('status')}" for name, payload in summary.get("sources", {}).items())
    )
    lines.append("")
    cohort = summary.get("cohort", {})
    lines.append("Active cohort")
    lines.append(
        f"- id={cohort.get('cohort_id')} status={cohort.get('status')} winner={cohort.get('winner', {}).get('wallet_id') if cohort.get('winner') else 'pending'}"
    )
    if cohort:
        lines.append(
            f"- target_fills={cohort.get('target_fills')} progress={cohort.get('progress_percent', 0)}%"
        )
        scored_windows = cohort.get("scored_windows", {})
        if scored_windows:
            lines.append("- scored_windows=" + ", ".join(scored_windows.keys()))
        archived_winners = cohort.get("archived_winners", [])
        if archived_winners:
            lines.append("- recent_winners=" + ", ".join(item.get("wallet_id", "?") for item in archived_winners[:3]))
    leaderboard = cohort.get("leaderboard", [])
    if leaderboard:
        lines.append("")
        lines.append("Cohort leaderboard")
        for row in leaderboard[:5]:
            lines.append(
                f"- {row.get('wallet_id')} {row.get('label')} | equity={row.get('equity')} | trades={row.get('trade_count')} | last={row.get('last_action')}"
            )
    lines.append("")
    lines.append("Top 5 live bots")
    live_top5 = summary.get("live_top5", [])
    if live_top5:
        for row in live_top5[:5]:
            lines.append(
                f"- {row.get('wallet_id')} {row.get('label')} | promotion={row.get('promotion_score')} | equity={row.get('equity')}"
            )
    else:
        lines.append("- none promoted yet")
    lines.append("")
    lines.append("Top opportunities")
    for row in summary.get("top_opportunities", [])[:5]:
        lines.append(
            f"- {row.get('slug')} | buys={row.get('buy_signals')} | ev={row.get('avg_expected_value')}"
        )
    return "\n".join(lines)
