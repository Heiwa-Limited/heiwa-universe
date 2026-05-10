from __future__ import annotations

import asyncio
import sys
import types
from types import SimpleNamespace

import pytest

if "discord" not in sys.modules:
    discord_module = types.ModuleType("discord")

    class DummyView:
        def __init__(self, *args, **kwargs) -> None:
            self.children = []

        def add_item(self, item) -> None:
            self.children.append(item)

    def dummy_button(*args, **kwargs):
        def decorator(func):
            return func
        return decorator

    class DummyButtonStyle:
        success = 1
        danger = 2
        secondary = 3
        primary = 4
        link = 5

    class DummyIntents:
        @staticmethod
        def default():
            return SimpleNamespace(message_content=False, members=False)

    discord_module.ui = SimpleNamespace(View=DummyView, Button=object, button=dummy_button)
    discord_module.ButtonStyle = DummyButtonStyle
    discord_module.Intents = DummyIntents
    discord_module.Interaction = object
    discord_module.Message = object
    discord_module.Thread = type("Thread", (), {})

    app_commands_module = types.ModuleType("discord.app_commands")
    app_commands_module.command = dummy_button
    app_commands_module.checks = SimpleNamespace(has_permissions=dummy_button)
    discord_module.app_commands = app_commands_module

    commands_module = types.ModuleType("discord.ext.commands")

    class DummyTree:
        def command(self, *args, **kwargs):
            return dummy_button(*args, **kwargs)

        async def sync(self):
            return []

    class DummyBot:
        def __init__(self, *args, **kwargs) -> None:
            self.user = object()
            self.tree = DummyTree()

        def event(self, func):
            return func

        def get_channel(self, channel_id):
            return None

    commands_module.Bot = DummyBot

    ext_module = types.ModuleType("discord.ext")
    ext_module.commands = commands_module

    sys.modules["discord"] = discord_module
    sys.modules["discord.app_commands"] = app_commands_module
    sys.modules["discord.ext"] = ext_module
    sys.modules["discord.ext.commands"] = commands_module

from apps.heiwa_hub.agents.heiwaclaw import HeiwaClawAgent
from apps.heiwa_hub.agents.messenger import MessengerAgent
from heiwa_protocol.protocol import Subject


class FakeAuthor:
    def __init__(self, user_id: int, name: str) -> None:
        self.id = user_id
        self.name = name
        self.bot = False

    def __str__(self) -> str:
        return self.name


class FakeMessage:
    def __init__(self, *, author: FakeAuthor, content: str, channel_id: int) -> None:
        self.author = author
        self.content = content
        self.guild = None
        self.channel = SimpleNamespace(id=channel_id)
        self.attachments = []
        self.embeds = []


@pytest.mark.asyncio
async def test_dm_fast_path_uses_canonical_user_and_stable_session(monkeypatch):
    from apps.heiwa_hub.agents import messenger as messenger_module

    ensure_calls: list[tuple[object, dict]] = []
    published: list[tuple[str, dict]] = []
    scheduled: list[asyncio.Task] = []

    def fake_ensure_user(stdb, discord_data: dict) -> str:
        ensure_calls.append((stdb, discord_data))
        return "user-alpha"

    async def fake_publish(subject: str, payload: dict) -> None:
        published.append((subject, payload))

    async def fake_track_identity(message) -> None:
        return None

    real_create_task = asyncio.create_task

    def capture_task(coro):
        task = real_create_task(coro)
        scheduled.append(task)
        return task

    monkeypatch.setattr(messenger_module, "ensure_user", fake_ensure_user, raising=False)
    monkeypatch.setattr(messenger_module.asyncio, "create_task", capture_task)

    agent = MessengerAgent.__new__(MessengerAgent)
    agent.bot = SimpleNamespace(user=object())
    agent.db = SimpleNamespace(stdb=object())
    agent._extract_full_content = lambda message: message.content
    agent._publish_raw = fake_publish
    agent._track_identity = fake_track_identity

    message = FakeMessage(author=FakeAuthor(123, "devon"), content="hello", channel_id=456)

    await MessengerAgent.on_message(agent, message)
    if scheduled:
        await asyncio.gather(*scheduled)

    assert ensure_calls == [
        (
            agent.db.stdb,
            {
                "discord_user_id": "123",
                "username": "devon",
                "bootstrap_source": "discord_dm",
            },
        )
    ]
    assert published == [
        (
            Subject.HEIWA_AGENT_INGRESS.value,
            {
                "content": "hello",
                "author": "devon",
                "owner_id": "user-alpha",
                "principal_id": "discord:123",
                "session_id": "discord-dm-123",
                "channel_id": 456,
            },
        )
    ]


@pytest.mark.asyncio
async def test_heiwaclaw_dm_uses_payload_identity(monkeypatch):
    import importlib

    chat_module = importlib.import_module("heiwa_hub.chat")

    calls: list[dict] = []
    dms: list[str] = []

    class FakeEngine:
        async def respond(self, *, session_id: str, content: str, author: str, owner_id: str) -> str:
            calls.append(
                {
                    "session_id": session_id,
                    "content": content,
                    "author": author,
                    "owner_id": owner_id,
                }
            )
            return "reply"

    monkeypatch.setattr(chat_module, "get_chat_engine", lambda: FakeEngine())

    agent = HeiwaClawAgent.__new__(HeiwaClawAgent)
    agent._store_operator_message = lambda *args, **kwargs: None

    async def fake_dm(message: str) -> None:
        dms.append(message)

    agent._dm = fake_dm

    await HeiwaClawAgent._on_direct_dm(
        agent,
        {
            "data": {
                "content": "ship it",
                "author": "devon",
                "owner_id": "user-alpha",
                "principal_id": "discord:123",
                "session_id": "discord-dm-123",
            }
        },
    )

    assert calls == [
        {
            "session_id": "discord-dm-123",
            "content": "ship it",
            "author": "devon",
            "owner_id": "user-alpha",
        }
    ]
    assert dms == ["reply"]
