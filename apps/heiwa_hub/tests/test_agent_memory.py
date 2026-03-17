"""Tests for AgentMemory STDB bridge and service layer."""
import pytest
from unittest.mock import MagicMock, patch
import uuid
import time


class TestSpacetimeDBBridge:
    """Test the STDB bridge methods for captain memory tables."""

    def test_insert_captain_message_calls_reducer(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.call = MagicMock(return_value=True)

        result = stdb.insert_captain_message(
            message_id="msg-1",
            session_id="sess-1",
            role="operator",
            content="hello",
            timestamp=1710000000000,
            source="discord_dm",
        )
        assert result is True
        stdb.call.assert_called_once_with(
            "insert_captain_message",
            "msg-1", "sess-1", "operator", "hello", 1710000000000, "discord_dm",
        )

    def test_get_uncompressed_messages_queries(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.query = MagicMock(return_value=[
            {"message_id": "m1", "role": "operator", "content": "hi", "timestamp": 100},
        ])

        result = stdb.get_uncompressed_messages(session_id="sess-1", limit=50)
        assert len(result) == 1
        assert result[0]["message_id"] == "m1"
        stdb.query.assert_called_once()
        assert "compressed = false" in stdb.query.call_args[0][0]

    def test_mark_messages_compressed_calls_reducer(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.call = MagicMock(return_value=True)

        result = stdb.mark_messages_compressed(session_id="sess-1", before_timestamp=999)
        assert result is True
        stdb.call.assert_called_once_with("mark_messages_compressed", "sess-1", 999)

    def test_insert_captain_summary_calls_reducer(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.call = MagicMock(return_value=True)

        result = stdb.insert_captain_summary(
            summary_id="sum-1",
            summary_type="rolling",
            content="summary text",
            range_start=100,
            range_end=500,
            messages_compressed=10,
        )
        assert result is True
        stdb.call.assert_called_once_with(
            "insert_captain_summary",
            "sum-1", "rolling", "summary text", 100, 500, 10,
        )

    def test_get_recent_summaries_queries(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.query = MagicMock(return_value=[
            {"summary_id": "s1", "content": "day recap", "created_at": 200},
        ])

        result = stdb.get_recent_summaries(limit=3)
        assert len(result) == 1
        stdb.query.assert_called_once()

    def test_get_active_focuses_queries(self):
        from heiwa_sdk.spacetimedb import SpacetimeDB
        stdb = SpacetimeDB.__new__(SpacetimeDB)
        stdb.db_identity = "test"
        stdb.server = "local"
        stdb.query = MagicMock(return_value=[
            {"focus_id": "f1", "topic": "deployment", "priority": 3},
        ])

        result = stdb.get_active_focuses()
        assert len(result) == 1
        assert result[0]["topic"] == "deployment"
        stdb.query.assert_called_once()
        assert "resolved_at = 0" in stdb.query.call_args[0][0]


class TestAgentMemory:
    """Test the AgentMemory high-level service."""

    def _make_memory(self):
        from heiwa_sdk.agent_memory import AgentMemory
        mem = AgentMemory.__new__(AgentMemory)
        mem.stdb = MagicMock()
        mem.session_id = "test-session"
        mem._token_budget = 8000
        return mem

    def test_store_message(self):
        from unittest.mock import ANY
        mem = self._make_memory()
        mem.stdb.insert_captain_message = MagicMock(return_value=True)

        result = mem.store_message(role="operator", content="deploy now", source="discord_dm")
        assert result is True
        mem.stdb.insert_captain_message.assert_called_once_with(
            message_id=ANY,
            session_id="test-session",
            role="operator",
            content="deploy now",
            timestamp=ANY,
            source="discord_dm",
        )

    def test_load_context_window(self):
        mem = self._make_memory()
        mem.stdb.get_uncompressed_messages = MagicMock(return_value=[
            {"message_id": "m1", "role": "operator", "content": "hi", "timestamp": 100},
            {"message_id": "m2", "role": "agent", "content": "hello", "timestamp": 200},
        ])
        mem.stdb.get_active_focuses = MagicMock(return_value=[])
        mem.stdb.get_recent_summaries = MagicMock(return_value=[])

        ctx = mem.load_context_window()
        assert len(ctx["messages"]) == 2
        assert ctx["messages"][0]["role"] == "operator"

    def test_needs_compression_under_budget(self):
        mem = self._make_memory()
        # ~10 chars = ~2 tokens, well under 8000
        messages = [{"content": "short msg"}]
        assert mem.needs_compression(messages) is False

    def test_needs_compression_over_budget(self):
        mem = self._make_memory()
        # Each message ~128K chars = ~32K tokens, over 8K budget
        messages = [{"content": "x" * 128000}]
        assert mem.needs_compression(messages) is True

    def test_estimate_tokens(self):
        from heiwa_sdk.agent_memory import AgentMemory
        assert AgentMemory.estimate_tokens("hello world") == 2  # 11 chars // 4 = 2
        assert AgentMemory.estimate_tokens("x" * 100) == 25  # 100 // 4

    def test_detect_complexity_simple(self):
        mem = self._make_memory()
        assert mem.detect_complexity("how's the deploy going?") is False

    def test_detect_complexity_architecture(self):
        mem = self._make_memory()
        assert mem.detect_complexity(
            "I want to refactor the entire deployment pipeline and redesign the strategy for multi-node orchestration"
        ) is True
