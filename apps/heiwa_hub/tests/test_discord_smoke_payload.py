from __future__ import annotations

from apps.heiwa_hub.actions import smoke_test_discord


def test_discord_smoke_payload_targets_the_response_channel():
    payload = smoke_test_discord._build_smoke_payload(
        task_id="smoke-discord-abc123",
        probe_id="probe123",
        channel_id=999001,
    )

    assert payload["task_id"] == "smoke-discord-abc123"
    assert payload["source_surface"] == "discord"
    assert payload["source_channel_id"] == 999001
    assert payload["response_channel_id"] == 999001
