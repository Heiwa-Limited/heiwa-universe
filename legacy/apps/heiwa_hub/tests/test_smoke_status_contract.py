from __future__ import annotations

from apps.heiwa_hub.actions import smoke_test, smoke_test_discord


def test_smoke_allows_direct_terminal_success_when_progress_events_are_missed():
    statuses = ["PASS"]

    assert smoke_test._has_required_progress(statuses) is True
    assert smoke_test_discord._has_required_progress(statuses) is True


def test_smoke_rejects_status_streams_without_progress_or_terminal_success():
    statuses = ["QUEUED"]

    assert smoke_test._has_required_progress(statuses) is False
    assert smoke_test_discord._has_required_progress(statuses) is False
