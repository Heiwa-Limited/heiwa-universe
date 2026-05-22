from __future__ import annotations

import argparse
import json
import time
from datetime import datetime, timezone
from pathlib import Path
from uuid import uuid4

from heiwa_trading.cockpit import (
    CockpitServerConfig,
    DEFAULT_COCKPIT_HOST,
    DEFAULT_COCKPIT_PORT,
    build_cockpit_snapshot,
    serve_cockpit,
)

# cockpit_service is a Mac-local module.
try:
    from heiwa_trading.cockpit_service import (
        install_service as install_cockpit_service,
        service_status as cockpit_service_status,
        start_service as start_cockpit_service,
        stop_service as stop_cockpit_service,
        uninstall_service as uninstall_cockpit_service,
    )
except ImportError:
    install_cockpit_service = None
    cockpit_service_status = None
    start_cockpit_service = None
    stop_cockpit_service = None
    uninstall_cockpit_service = None

from heiwa_trading.coinmarketcap import fetch_coinmarketcap_summary
from heiwa_trading.config import CHEAP_POLICY, DEFAULT_TOURNAMENT_LIMIT, DEFAULT_TOURNAMENT_SLEEP_SECONDS

# live_dashboard is a Mac-local module.
try:
    from heiwa_trading.live_dashboard import build_live_dashboard_text, build_supervisor_dashboard_text
except ImportError:
    build_live_dashboard_text = None
    build_supervisor_dashboard_text = None
from heiwa_trading.market_data import fetch_markets, get_market_by_slug
from heiwa_trading.paper_trader import PaperTrade, load_portfolio, save_portfolio
from heiwa_trading.scan import append_scan_history, build_scan_summary, build_tuning_report, load_scan_history, save_tuning_report
from heiwa_trading.strategy import score_market
from heiwa_trading.supervisor import (
    DEFAULT_HOLD_WINDOWS,
    DEFAULT_TARGET_FILLS,
    build_supervisor_summary,
    create_supervisor_state,
    leaderboard_top5,
    load_supervisor_state,
    save_supervisor_state,
    supervisor_tick,
)
from heiwa_trading.tournament import (
    TournamentState,
    build_default_variants,
    leaderboard_rows,
    load_tournament_state,
    run_tournament_step,
    save_tournament_state,
)


def _parse_sources(raw_value: str) -> tuple[str, ...]:
    sources = tuple(part.strip() for part in raw_value.split(",") if part.strip())
    return sources or ("polymarket",)


def _run_scan_cycle(*, limit: int, sources: tuple[str, ...], history_limit: int = 50) -> tuple[TournamentState, dict[str, object], dict[str, object]]:
    state = load_tournament_state()
    timestamp = datetime.now(timezone.utc).isoformat()
    source_details: dict[str, dict[str, object]] = {}
    if "coinmarketcap" in sources:
        source_details["coinmarketcap"] = fetch_coinmarketcap_summary(limit=limit)
    markets = fetch_markets(limit=limit) if "polymarket" in sources else []
    state, summary = build_scan_summary(
        state=state,
        markets=markets,
        sources=sources,
        timestamp=timestamp,
        source_details=source_details,
    )
    save_tournament_state(state)
    append_scan_history(summary)
    history = load_scan_history(limit=history_limit)
    report = build_tuning_report(state=state, history=history, timestamp=timestamp)
    save_tuning_report(report)
    return state, summary, report


def _fetch_source_context(*, limit: int, sources: tuple[str, ...]) -> tuple[list, dict[str, dict[str, object]], dict[str, dict[str, object]]]:
    snapshots: dict[str, dict[str, object]] = {}
    statuses: dict[str, dict[str, object]] = {}
    markets = fetch_markets(limit=limit) if "polymarket" in sources else []
    if "polymarket" in sources:
        statuses["polymarket"] = {
            "status": "ok",
            "message": "Fetched public Polymarket market data.",
            "markets": len(markets),
        }
    if "coinmarketcap" in sources:
        snapshot = fetch_coinmarketcap_summary(limit=limit)
        snapshots["coinmarketcap"] = snapshot
        statuses["coinmarketcap"] = {
            "status": snapshot.get("status", "unknown"),
            "message": snapshot.get("message", ""),
            "asset_count": snapshot.get("asset_count", 0),
            "cached": snapshot.get("cached", False),
        }
    return markets, statuses, snapshots


def _run_supervisor_cycle(
    *,
    limit: int,
    sources: tuple[str, ...],
    target_fills: int,
    hold_windows: tuple[int, ...],
) -> tuple[object, dict[str, object]]:
    state = load_supervisor_state()
    timestamp = datetime.now(timezone.utc).isoformat()
    markets, source_statuses, source_snapshots = _fetch_source_context(limit=limit, sources=sources)
    next_state, summary = supervisor_tick(
        state=state,
        markets=markets,
        timestamp=timestamp,
        target_fills=target_fills,
        hold_windows=hold_windows,
        source_statuses=source_statuses,
        source_snapshots=source_snapshots,
    )
    save_supervisor_state(next_state)
    return next_state, summary


def _parse_hold_windows(raw_value: str) -> tuple[int, ...]:
    units = {"m": 60, "h": 3600}
    windows = []
    for part in (item.strip().lower() for item in raw_value.split(",") if item.strip()):
        unit = part[-1]
        if unit not in units:
            raise SystemExit(f"Unsupported hold window '{part}'. Use values like 5m,15m,1h,4h.")
        windows.append(int(part[:-1]) * units[unit])
    return tuple(windows) or DEFAULT_HOLD_WINDOWS


def command_markets(args: argparse.Namespace) -> int:
    markets = fetch_markets(limit=args.limit)
    for market in markets:
        print(f"{market.slug}\t{market.yes_price:.3f}\t{market.question}")
    return 0


def command_score(args: argparse.Namespace) -> int:
    market = get_market_by_slug(args.slug)
    decision = score_market(
        market=market,
        subjective_probability=args.probability,
        policy=CHEAP_POLICY,
    )
    print(json.dumps(decision.to_dict(), indent=2))
    return 0


def command_paper_buy(args: argparse.Namespace) -> int:
    market = get_market_by_slug(args.slug)
    decision = score_market(
        market=market,
        subjective_probability=args.probability,
        policy=CHEAP_POLICY,
    )
    if decision.action != "BUY":
        print(json.dumps(decision.to_dict(), indent=2))
        return 0

    portfolio = load_portfolio(policy=CHEAP_POLICY)
    trade = PaperTrade(
        trade_id=str(uuid4()),
        slug=market.slug,
        side="YES",
        price=market.yes_price,
        probability=decision.posterior_probability,
        notional=decision.recommended_notional,
        timestamp=datetime.now(timezone.utc).isoformat(),
    )
    portfolio = portfolio.apply_trade(trade)
    save_portfolio(portfolio)
    print(json.dumps({"trade": trade.__dict__, "portfolio": portfolio.to_dict()}, indent=2))
    return 0


def command_portfolio(args: argparse.Namespace) -> int:
    portfolio = load_portfolio(policy=CHEAP_POLICY)
    print(json.dumps(portfolio.to_dict(), indent=2))
    return 0


def command_tournament_init(args: argparse.Namespace) -> int:
    state = TournamentState.empty(policy=CHEAP_POLICY, variants=build_default_variants())
    save_tournament_state(state)
    print(json.dumps({"tick": state.tick, "wallets": len(state.wallets)}, indent=2))
    return 0


def command_tournament_step(args: argparse.Namespace) -> int:
    state, _, report = _run_scan_cycle(limit=args.limit, sources=("polymarket",))
    print(
        json.dumps(
            {
                "tick": state.tick,
                "leaderboard": leaderboard_rows(state),
                "recommended_wallet_id": report["recommended_wallet_id"],
            },
            indent=2,
        )
    )
    return 0


def command_tournament_leaderboard(args: argparse.Namespace) -> int:
    state = load_tournament_state()
    print(json.dumps({"tick": state.tick, "leaderboard": leaderboard_rows(state)}, indent=2))
    return 0


def command_tournament_wallet(args: argparse.Namespace) -> int:
    state = load_tournament_state()
    wallet = next((wallet for wallet in state.wallets if wallet.wallet_id == args.wallet_id), None)
    if wallet is None:
        raise SystemExit(f"Unknown wallet id: {args.wallet_id}")
    payload = {
        "wallet_id": wallet.wallet_id,
        "label": wallet.label,
        "trade_count": wallet.trade_count,
        "unrealized_pnl": wallet.unrealized_pnl,
        "last_action": wallet.last_action,
        "portfolio": wallet.portfolio.to_dict(),
    }
    print(json.dumps(payload, indent=2))
    return 0


def command_tournament_loop(args: argparse.Namespace) -> int:
    state = load_tournament_state()
    for _ in range(args.iterations):
        state, summary, report = _run_scan_cycle(limit=args.limit, sources=("polymarket",))
        print(
            json.dumps(
                {
                    "tick": state.tick,
                    "top_opportunities": summary["top_opportunities"][:3],
                    "leaderboard": leaderboard_rows(state)[:3],
                    "recommended_wallet_id": report["recommended_wallet_id"],
                },
                indent=2,
            )
        )
        if _ != args.iterations - 1:
            time.sleep(args.sleep_seconds)
    return 0


def command_scan(args: argparse.Namespace) -> int:
    _, summary, report = _run_scan_cycle(
        limit=args.limit,
        sources=_parse_sources(args.sources),
        history_limit=args.history_limit,
    )
    payload = {
        "summary": summary,
        "recommended_wallet_id": report["recommended_wallet_id"],
        "recommended_label": report["recommended_label"],
        "ticks_observed": report["ticks_observed"],
    }
    print(json.dumps(payload, indent=2))
    return 0


def command_history(args: argparse.Namespace) -> int:
    print(json.dumps({"history": load_scan_history(limit=args.limit)}, indent=2))
    return 0


def command_report(args: argparse.Namespace) -> int:
    state = load_tournament_state()
    history = load_scan_history(limit=args.history_limit)
    report = build_tuning_report(
        state=state,
        history=history,
        timestamp=datetime.now(timezone.utc).isoformat(),
    )
    save_tuning_report(report)
    print(json.dumps(report, indent=2))
    return 0


def command_supervisor_init(args: argparse.Namespace) -> int:
    state = create_supervisor_state(policy=CHEAP_POLICY)
    save_supervisor_state(state)
    print(json.dumps({"status": "initialized"}, indent=2))
    return 0


def command_supervisor_status(args: argparse.Namespace) -> int:
    print(json.dumps(build_supervisor_summary(load_supervisor_state()), indent=2))
    return 0


def command_supervisor_tick(args: argparse.Namespace) -> int:
    _, summary = _run_supervisor_cycle(
        limit=args.limit,
        sources=_parse_sources(args.sources),
        target_fills=args.target_fills,
        hold_windows=_parse_hold_windows(args.hold_windows),
    )
    print(json.dumps(summary, indent=2))
    return 0


def command_supervisor_loop(args: argparse.Namespace) -> int:
    iteration = 0
    while args.iterations == 0 or iteration < args.iterations:
        _, summary = _run_supervisor_cycle(
            limit=args.limit,
            sources=_parse_sources(args.sources),
            target_fills=args.target_fills,
            hold_windows=_parse_hold_windows(args.hold_windows),
        )
        payload = {
            "timestamp": summary.get("timestamp"),
            "cohort_status": summary.get("cohort", {}).get("status") if summary.get("cohort") else None,
            "winner": summary.get("cohort", {}).get("winner"),
            "live_top5": summary.get("live_top5", [])[:5],
        }
        print(json.dumps(payload, indent=2), flush=True)
        iteration += 1
        if args.iterations != 0 and iteration >= args.iterations:
            return 0
        time.sleep(args.interval_seconds)


def command_supervisor_top5(args: argparse.Namespace) -> int:
    state = load_supervisor_state()
    print(json.dumps({"live_top5": leaderboard_top5(state)}, indent=2))
    return 0


def command_supervisor_monitor(args: argparse.Namespace) -> int:
    if build_supervisor_dashboard_text is None:
        raise SystemExit("build_supervisor_dashboard_text module not available on this platform")
    iteration = 0
    while args.iterations == 0 or iteration < args.iterations:
        summary = build_supervisor_summary(load_supervisor_state())
        print("\033[2J\033[H" + build_supervisor_dashboard_text(summary), end="", flush=True)
        iteration += 1
        if args.iterations != 0 and iteration >= args.iterations:
            print()
            return 0
        time.sleep(args.refresh_seconds)


def command_supervisor_leaderboard(args: argparse.Namespace) -> int:
    summary = build_supervisor_summary(load_supervisor_state())
    print(json.dumps({"cohort": summary.get("cohort"), "live_top5": summary.get("live_top5")}, indent=2))
    return 0


def command_cockpit_snapshot(args: argparse.Namespace) -> int:
    print(json.dumps(build_cockpit_snapshot(Path(args.root_dir)), indent=2))
    return 0


def command_cockpit_serve(args: argparse.Namespace) -> int:
    return serve_cockpit(
        CockpitServerConfig(
            root_dir=Path(args.root_dir),
            host=args.host,
            port=args.port,
            event_interval_seconds=args.event_interval_seconds,
            open_browser=not args.no_open,
        )
    )


def command_cockpit_service(args: argparse.Namespace) -> int:
    if install_cockpit_service is None:
        raise SystemExit("cockpit_service module not available on this platform (mac-agent only)")
    if args.cockpit_service_command == "install":
        payload = install_cockpit_service(port=args.port, host=args.host)
    elif args.cockpit_service_command == "start":
        payload = start_cockpit_service()
    elif args.cockpit_service_command == "stop":
        payload = stop_cockpit_service()
    elif args.cockpit_service_command == "status":
        payload = cockpit_service_status()
    elif args.cockpit_service_command == "uninstall":
        payload = uninstall_cockpit_service()
    else:  # pragma: no cover
        raise SystemExit(f"Unknown cockpit service command {args.cockpit_service_command}")
    print(json.dumps(payload, indent=2))
    return 0


def command_monitor(args: argparse.Namespace) -> int:
    if build_live_dashboard_text is None:
        raise SystemExit("live_dashboard module not available on this platform (mac-agent only)")
    iteration = 0
    while args.iterations == 0 or iteration < args.iterations:
        _, summary, report = _run_scan_cycle(
            limit=args.limit,
            sources=_parse_sources(args.sources),
            history_limit=args.history_limit,
        )
        print("\033[2J\033[H" + build_live_dashboard_text(summary=summary, report=report), end="", flush=True)
        iteration += 1
        if args.iterations != 0 and iteration >= args.iterations:
            print()
            return 0
        time.sleep(args.interval_seconds)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Local-first Polymarket foundation CLI")
    subparsers = parser.add_subparsers(dest="command", required=True)

    markets_parser = subparsers.add_parser("markets", help="Fetch active markets")
    markets_parser.add_argument("--limit", type=int, default=10)
    markets_parser.set_defaults(func=command_markets)

    score_parser = subparsers.add_parser("score", help="Score a market using the cheap policy")
    score_parser.add_argument("--slug", required=True)
    score_parser.add_argument("--probability", type=float, required=True)
    score_parser.set_defaults(func=command_score)

    paper_buy_parser = subparsers.add_parser("paper-buy", help="Write a simulated buy if the signal clears policy")
    paper_buy_parser.add_argument("--slug", required=True)
    paper_buy_parser.add_argument("--probability", type=float, required=True)
    paper_buy_parser.set_defaults(func=command_paper_buy)

    portfolio_parser = subparsers.add_parser("portfolio", help="Show the simulated portfolio")
    portfolio_parser.set_defaults(func=command_portfolio)

    scan_parser = subparsers.add_parser("scan", help="Run one market scan and persist tournament history")
    scan_parser.add_argument("--limit", type=int, default=DEFAULT_TOURNAMENT_LIMIT)
    scan_parser.add_argument("--sources", default="polymarket,coinmarketcap")
    scan_parser.add_argument("--history-limit", type=int, default=50)
    scan_parser.set_defaults(func=command_scan)

    history_parser = subparsers.add_parser("history", help="Show recent persisted scan history")
    history_parser.add_argument("--limit", type=int, default=10)
    history_parser.set_defaults(func=command_history)

    report_parser = subparsers.add_parser("report", help="Build a current tuning report from scan history")
    report_parser.add_argument("--history-limit", type=int, default=50)
    report_parser.set_defaults(func=command_report)

    monitor_parser = subparsers.add_parser("monitor", help="Run a live terminal view of the tournament")
    monitor_parser.add_argument("--limit", type=int, default=DEFAULT_TOURNAMENT_LIMIT)
    monitor_parser.add_argument("--sources", default="polymarket,coinmarketcap")
    monitor_parser.add_argument("--history-limit", type=int, default=50)
    monitor_parser.add_argument("--interval-seconds", type=int, default=30)
    monitor_parser.add_argument("--iterations", type=int, default=0)
    monitor_parser.set_defaults(func=command_monitor)

    supervisor_parser = subparsers.add_parser("supervisor", help="Run the cohort/top-5 market supervisor")
    supervisor_subparsers = supervisor_parser.add_subparsers(dest="supervisor_command", required=True)

    supervisor_init_parser = supervisor_subparsers.add_parser("init", help="Initialize supervisor state")
    supervisor_init_parser.set_defaults(func=command_supervisor_init)

    supervisor_status_parser = supervisor_subparsers.add_parser("status", help="Show current supervisor status")
    supervisor_status_parser.set_defaults(func=command_supervisor_status)

    supervisor_tick_parser = supervisor_subparsers.add_parser("tick", help="Advance the supervisor by one scan cycle")
    supervisor_tick_parser.add_argument("--limit", type=int, default=DEFAULT_TOURNAMENT_LIMIT)
    supervisor_tick_parser.add_argument("--sources", default="polymarket,coinmarketcap")
    supervisor_tick_parser.add_argument("--target-fills", type=int, default=DEFAULT_TARGET_FILLS)
    supervisor_tick_parser.add_argument("--hold-windows", default="5m,15m,1h,4h")
    supervisor_tick_parser.set_defaults(func=command_supervisor_tick)

    supervisor_loop_parser = supervisor_subparsers.add_parser("loop", help="Run the supervisor loop continuously")
    supervisor_loop_parser.add_argument("--limit", type=int, default=DEFAULT_TOURNAMENT_LIMIT)
    supervisor_loop_parser.add_argument("--sources", default="polymarket,coinmarketcap")
    supervisor_loop_parser.add_argument("--target-fills", type=int, default=DEFAULT_TARGET_FILLS)
    supervisor_loop_parser.add_argument("--hold-windows", default="5m,15m,1h,4h")
    supervisor_loop_parser.add_argument("--interval-seconds", type=int, default=60)
    supervisor_loop_parser.add_argument("--iterations", type=int, default=0)
    supervisor_loop_parser.set_defaults(func=command_supervisor_loop)

    supervisor_top5_parser = supervisor_subparsers.add_parser("top5", help="Show the promoted top-5 live bots")
    supervisor_top5_parser.set_defaults(func=command_supervisor_top5)

    supervisor_monitor_parser = supervisor_subparsers.add_parser("monitor", help="Render the supervisor live board")
    supervisor_monitor_parser.add_argument("--refresh-seconds", type=int, default=5)
    supervisor_monitor_parser.add_argument("--iterations", type=int, default=0)
    supervisor_monitor_parser.set_defaults(func=command_supervisor_monitor)

    supervisor_leaderboard_parser = supervisor_subparsers.add_parser("leaderboard", help="Show cohort and top-5 leaderboards")
    supervisor_leaderboard_parser.set_defaults(func=command_supervisor_leaderboard)

    cockpit_parser = subparsers.add_parser("cockpit", help="Serve the local trading cockpit")
    cockpit_subparsers = cockpit_parser.add_subparsers(dest="cockpit_command", required=True)

    cockpit_snapshot_parser = cockpit_subparsers.add_parser("snapshot", help="Print the current cockpit snapshot")
    cockpit_snapshot_parser.add_argument("--root-dir", default=str(Path(__file__).resolve().parents[4]))
    cockpit_snapshot_parser.set_defaults(func=command_cockpit_snapshot)

    cockpit_serve_parser = cockpit_subparsers.add_parser("serve", help="Run the local cockpit web server")
    cockpit_serve_parser.add_argument("--root-dir", default=str(Path(__file__).resolve().parents[4]))
    cockpit_serve_parser.add_argument("--host", default=DEFAULT_COCKPIT_HOST)
    cockpit_serve_parser.add_argument("--port", type=int, default=DEFAULT_COCKPIT_PORT)
    cockpit_serve_parser.add_argument("--event-interval-seconds", type=int, default=3)
    cockpit_serve_parser.add_argument("--no-open", action="store_true")
    cockpit_serve_parser.set_defaults(func=command_cockpit_serve)

    cockpit_service_parser = subparsers.add_parser("cockpit-service", help="Manage the cockpit LaunchAgent")
    cockpit_service_subparsers = cockpit_service_parser.add_subparsers(dest="cockpit_service_command", required=True)

    cockpit_service_install = cockpit_service_subparsers.add_parser("install", help="Install and start the cockpit LaunchAgent")
    cockpit_service_install.add_argument("--port", type=int, default=DEFAULT_COCKPIT_PORT)
    cockpit_service_install.add_argument("--host", default=DEFAULT_COCKPIT_HOST)
    cockpit_service_install.set_defaults(func=command_cockpit_service)

    cockpit_service_start = cockpit_service_subparsers.add_parser("start", help="Start the cockpit LaunchAgent")
    cockpit_service_start.set_defaults(func=command_cockpit_service)

    cockpit_service_stop = cockpit_service_subparsers.add_parser("stop", help="Stop the cockpit LaunchAgent")
    cockpit_service_stop.set_defaults(func=command_cockpit_service)

    cockpit_service_status = cockpit_service_subparsers.add_parser("status", help="Show cockpit LaunchAgent status")
    cockpit_service_status.set_defaults(func=command_cockpit_service)

    cockpit_service_uninstall = cockpit_service_subparsers.add_parser("uninstall", help="Unload the cockpit LaunchAgent")
    cockpit_service_uninstall.set_defaults(func=command_cockpit_service)

    tournament_parser = subparsers.add_parser("tournament", help="Run the ten-wallet tournament simulator")
    tournament_subparsers = tournament_parser.add_subparsers(dest="tournament_command", required=True)

    tournament_init_parser = tournament_subparsers.add_parser("init", help="Initialize tournament state")
    tournament_init_parser.set_defaults(func=command_tournament_init)

    tournament_step_parser = tournament_subparsers.add_parser("step", help="Advance the tournament by one tick")
    tournament_step_parser.add_argument("--limit", type=int, default=DEFAULT_TOURNAMENT_LIMIT)
    tournament_step_parser.set_defaults(func=command_tournament_step)

    tournament_leaderboard_parser = tournament_subparsers.add_parser("leaderboard", help="Show ranked wallet results")
    tournament_leaderboard_parser.set_defaults(func=command_tournament_leaderboard)

    tournament_wallet_parser = tournament_subparsers.add_parser("wallet", help="Inspect one wallet")
    tournament_wallet_parser.add_argument("--wallet-id", required=True)
    tournament_wallet_parser.set_defaults(func=command_tournament_wallet)

    tournament_loop_parser = tournament_subparsers.add_parser("loop", help="Run repeated tournament ticks")
    tournament_loop_parser.add_argument("--iterations", type=int, default=1)
    tournament_loop_parser.add_argument("--limit", type=int, default=DEFAULT_TOURNAMENT_LIMIT)
    tournament_loop_parser.add_argument("--sleep-seconds", type=int, default=DEFAULT_TOURNAMENT_SLEEP_SECONDS)
    tournament_loop_parser.set_defaults(func=command_tournament_loop)

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
