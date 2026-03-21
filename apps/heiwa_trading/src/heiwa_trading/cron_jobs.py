from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class MarketScanCronJob:
    name: str
    description: str
    cron: str
    tz: str
    message: str
    session: str = "isolated"
    thinking: str = "minimal"
    timeout_seconds: int = 180
    announce: bool = True
    exact: bool = True
    light_context: bool = True


def _scan_message(root_dir: str, *, limit: int) -> str:
    return (
        f"From {root_dir}, run ./bin/mac-agent polymarket supervisor tick --sources polymarket,coinmarketcap --limit {limit} "
        "and then ./bin/mac-agent polymarket supervisor leaderboard. Return a concise operator summary with the "
        "active cohort status, the current top live bots, the best opportunity, and whether CoinMarketCap is healthy."
    )


def desired_market_scan_jobs(
    *,
    root_dir: str,
    timezone: str = "America/Vancouver",
    limit: int = 25,
) -> tuple[MarketScanCronJob, ...]:
    message = _scan_message(root_dir, limit=limit)
    return (
        MarketScanCronJob(
            name="market-scan-0230",
            description="Run the pre-market local scan at 02:30 America/Vancouver.",
            cron="30 2 * * *",
            tz=timezone,
            message=message,
        ),
        MarketScanCronJob(
            name="market-scan-1530",
            description="Run the afternoon local scan at 15:30 America/Vancouver.",
            cron="30 15 * * *",
            tz=timezone,
            message=message,
        ),
    )


def plan_market_scan_job_changes(
    *,
    existing_jobs: list[dict[str, object]],
    desired_jobs: tuple[MarketScanCronJob, ...],
) -> list[dict[str, object]]:
    existing_by_name = {str(job.get("name", "")): job for job in existing_jobs}
    changes = []
    for job in desired_jobs:
        existing = existing_by_name.get(job.name)
        if existing is None:
            changes.append({"action": "add", "job": job})
            continue
        changes.append({"action": "edit", "id": existing["id"], "job": job})
    return changes
