from __future__ import annotations

import json

from heiwa_sdk.db import Database


class _FakeStdb:
    def __init__(self):
        self.calls: list[tuple[str, tuple[object, ...]]] = []

    def call(self, reducer_name: str, *args: object) -> bool:
        self.calls.append((reducer_name, args))
        return True


def test_upsert_discord_channel_bridges_to_stdb_reducer():
    db = object.__new__(Database)
    db.stdb = _FakeStdb()

    ok = Database.upsert_discord_channel(
        db,
        "central-comms",
        123456789,
        category_name="MISSION CONTROL",
    )

    assert ok is True
    assert db.stdb.calls == [
        (
            "register_discord_channel",
            (
                123456789,
                "central-comms",
                "central-comms",
                json.dumps({"category": "MISSION CONTROL"}),
            ),
        )
    ]


def test_upsert_discord_role_is_safe_compat_noop():
    db = object.__new__(Database)
    db.stdb = _FakeStdb()

    ok = Database.upsert_discord_role(db, "Heiwa Admin", 987654321)

    assert ok is True
    assert db.stdb.calls == []
