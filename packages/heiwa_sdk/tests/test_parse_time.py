"""Tests for heiwa_sdk.intent.parse_time — deterministic via fixed `now`."""

from __future__ import annotations

from datetime import datetime
from zoneinfo import ZoneInfo

from heiwa_sdk.intent.parse_time import parse_time

TZ = "America/Vancouver"
NOW = datetime(2026, 6, 12, 12, 0, tzinfo=ZoneInfo(TZ))  # a Friday, noon


def test_one_off_weekday_time():
    result = parse_time("remind me Friday 3pm to call mom", now=NOW, tz_name=TZ)
    assert result.rrule is None
    assert result.confidence >= 0.8
    dt = datetime.fromisoformat(result.dt)
    assert (dt.hour, dt.minute) == (15, 0)
    assert dt.weekday() == 4  # Friday
    assert dt >= NOW.replace(tzinfo=dt.tzinfo)


def test_recurring_with_at():
    result = parse_time("every monday at 9am check pipeline", now=NOW, tz_name=TZ)
    assert result.rrule is not None
    assert "FREQ=WEEKLY" in result.rrule
    assert "BYDAY=MO" in result.rrule
    assert "BYHOUR=9" in result.rrule
    dt = datetime.fromisoformat(result.dt)
    assert dt.weekday() == 0 and dt.hour == 9


def test_recurring_without_at_repairs_byhour():
    result = parse_time("every monday 9am check pipeline", now=NOW, tz_name=TZ)
    assert result.rrule is not None and "BYHOUR=9" in result.rrule


def test_tomorrow():
    result = parse_time("tomorrow at 7:30am dentist", now=NOW, tz_name=TZ)
    dt = datetime.fromisoformat(result.dt)
    assert dt.date().isoformat() == "2026-06-13"
    assert (dt.hour, dt.minute) == (7, 30)


def test_unparseable_returns_zero_confidence():
    result = parse_time("no temporal content here whatsoever", now=NOW, tz_name=TZ)
    assert result.confidence == 0.0
    assert result.dt is None and result.rrule is None


def test_empty_text():
    result = parse_time("   ", now=NOW, tz_name=TZ)
    assert result.confidence == 0.0


def test_timezone_is_attached():
    result = parse_time("friday 3pm", now=NOW, tz_name=TZ)
    assert result.tz == TZ
    assert datetime.fromisoformat(result.dt).tzinfo is not None
