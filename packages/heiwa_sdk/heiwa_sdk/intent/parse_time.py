"""Free text -> ParsedTime { dt, tz, rrule, confidence }.

Deterministic NL time parsing on top of `recurrent` (recurrence grammar ->
RRULE) and `dateparser` (one-off datetime long tail). No LLM call; when
confidence is low the caller decides whether to escalate to DREX.

Runs three ways:
  - library:    from heiwa_sdk.intent import parse_time
  - module:     python -m heiwa_sdk.intent.parse_time --text "..." --json
  - standalone: python .../parse_time.py --text "..." --json
The standalone path is what `heiwa schedule` subprocesses into, so this file
must not import anything else from heiwa_sdk.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import asdict, dataclass
from datetime import datetime
from typing import Optional


@dataclass
class ParsedTime:
    """One parse result. `dt` is the next concrete occurrence (ISO 8601,
    timezone-aware); `rrule` is set only for recurring intents."""

    dt: Optional[str]
    tz: str
    rrule: Optional[str]
    confidence: float
    matched_text: Optional[str]

    def to_json(self) -> str:
        return json.dumps(asdict(self))


def _local_tz_name() -> str:
    try:
        import tzlocal

        return str(tzlocal.get_localzone())
    except Exception:
        return "UTC"


def _ensure_aware(dt: datetime, tz_name: str) -> datetime:
    if dt.tzinfo is not None:
        return dt
    from zoneinfo import ZoneInfo

    return dt.replace(tzinfo=ZoneInfo(tz_name))


_TIME_OF_DAY = None  # compiled lazily


def _extract_time_of_day(text: str) -> Optional[tuple[int, int]]:
    """Find an explicit clock time ("3pm", "7:30am", "15:00") in the text.
    Used to repair RRULEs when `recurrent` drops the time (it requires the
    word "at": "every monday 9am" parses without BYHOUR, "at 9am" keeps it)."""
    global _TIME_OF_DAY
    import re

    if _TIME_OF_DAY is None:
        _TIME_OF_DAY = re.compile(
            r"\b(\d{1,2})(?::(\d{2}))?\s*(am|pm)\b|\b(\d{1,2}):(\d{2})\b", re.IGNORECASE
        )
    match = _TIME_OF_DAY.search(text)
    if not match:
        return None
    if match.group(3):  # am/pm form
        hour = int(match.group(1)) % 12
        if match.group(3).lower() == "pm":
            hour += 12
        return hour, int(match.group(2) or 0)
    hour, minute = int(match.group(4)), int(match.group(5))
    return (hour, minute) if hour < 24 and minute < 60 else None


def _try_recurrent(text: str, now: datetime, tz_name: str) -> Optional[ParsedTime]:
    """recurrent handles both recurrence ("every monday 9am" -> RRULE) and a
    decent share of one-off phrases ("friday 3pm" -> datetime)."""
    try:
        from recurrent.event_parser import RecurringEvent
    except ImportError:
        return None
    event = RecurringEvent(now_date=now.replace(tzinfo=None))
    try:
        result = event.parse(text)
    except Exception:
        return None
    if result is None:
        return None
    if isinstance(result, str):
        # RRULE text for a recurring intent; compute the next occurrence.
        if "BYHOUR" not in result:
            tod = _extract_time_of_day(text)
            if tod is not None:
                result = f"{result};BYHOUR={tod[0]};BYMINUTE={tod[1]}"
        next_dt: Optional[str] = None
        try:
            from dateutil import rrule as dr

            dtstart = event.dtstart or now.replace(tzinfo=None)
            rule = dr.rrulestr(result, dtstart=dtstart)
            nxt = rule.after(now.replace(tzinfo=None), inc=True)
            if nxt is not None:
                next_dt = _ensure_aware(nxt, tz_name).isoformat()
        except Exception:
            pass
        return ParsedTime(
            dt=next_dt,
            tz=tz_name,
            rrule=result,
            confidence=0.85,
            matched_text=getattr(event, "get_params", lambda: None)() and text or text,
        )
    if isinstance(result, datetime):
        has_time = (result.hour, result.minute) != (0, 0)
        return ParsedTime(
            dt=_ensure_aware(result, tz_name).isoformat(),
            tz=tz_name,
            rrule=None,
            confidence=0.9 if has_time else 0.7,
            matched_text=text,
        )
    return None


def _try_dateparser(text: str, now: datetime, tz_name: str) -> Optional[ParsedTime]:
    try:
        from dateparser.search import search_dates
    except ImportError:
        return None
    settings = {
        "PREFER_DATES_FROM": "future",
        "RELATIVE_BASE": now.replace(tzinfo=None),
        "RETURN_AS_TIMEZONE_AWARE": False,
    }
    try:
        found = search_dates(text, languages=["en"], settings=settings)
    except Exception:
        return None
    if not found:
        return None
    matched, dt = found[0]
    has_time = (dt.hour, dt.minute) != (0, 0)
    return ParsedTime(
        dt=_ensure_aware(dt, tz_name).isoformat(),
        tz=tz_name,
        rrule=None,
        confidence=0.8 if has_time else 0.6,
        matched_text=matched,
    )


def parse_time(text: str, now: Optional[datetime] = None, tz_name: Optional[str] = None) -> ParsedTime:
    """Parse free text into a ParsedTime. Never raises on unparseable input;
    returns confidence 0.0 instead so callers can escalate."""
    tz_name = tz_name or _local_tz_name()
    if now is None:
        from zoneinfo import ZoneInfo

        now = datetime.now(ZoneInfo(tz_name))
    text = text.strip()
    if not text:
        return ParsedTime(dt=None, tz=tz_name, rrule=None, confidence=0.0, matched_text=None)

    result = _try_recurrent(text, now, tz_name)
    if result is not None and (result.rrule or result.dt):
        return result
    result = _try_dateparser(text, now, tz_name)
    if result is not None:
        return result
    return ParsedTime(dt=None, tz=tz_name, rrule=None, confidence=0.0, matched_text=None)


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Parse free text into a structured time")
    parser.add_argument("--text", required=True, help="free text containing a time expression")
    parser.add_argument("--now", help="ISO datetime to use as 'now' (deterministic tests)")
    parser.add_argument("--tz", help="IANA timezone name override")
    parser.add_argument("--json", action="store_true", help="emit JSON (default)")
    args = parser.parse_args(argv)

    now = None
    if args.now:
        now = datetime.fromisoformat(args.now)
        if now.tzinfo is None:
            now = _ensure_aware(now, args.tz or _local_tz_name())
    result = parse_time(args.text, now=now, tz_name=args.tz)
    print(result.to_json())
    return 0


if __name__ == "__main__":
    sys.exit(main())
