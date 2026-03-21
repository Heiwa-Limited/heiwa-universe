"""Tests for the Chat Engine — session management, value extraction, cost cascade."""
import asyncio
import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from heiwa_hub.chat import ChatEngine, ChatSession, _ZERO_VALUE, _VALUE_PATTERNS
import re


class TestChatSession:
    def test_session_stores_messages(self):
        s = ChatSession(session_id="test")
        s.add("operator", "yo")
        s.add("agent", "hey, what's good?")
        assert len(s.messages) == 2
        assert s.messages[0]["role"] == "operator"
        assert s.messages[1]["content"] == "hey, what's good?"

    def test_session_trims_to_max_history(self):
        s = ChatSession(session_id="test")
        for i in range(30):
            s.add("operator", f"msg {i}")
        assert len(s.messages) == 20  # _MAX_HISTORY

    def test_format_history(self):
        s = ChatSession(session_id="test")
        s.add("operator", "hello")
        s.add("agent", "hey")
        out = s.format_history()
        assert "OPERATOR: hello" in out
        assert "AGENT: hey" in out


class TestValueExtraction:
    """Verify the regex patterns correctly classify chat value."""

    @pytest.mark.parametrize("msg", [
        "yo", "hey!", "gm", "sup", "thanks", "ok", "lol", "nice", "bet", "cool",
        "yooo", "hello!", "np", "gg", "ty", "aight",
    ])
    def test_zero_value_messages(self, msg):
        assert _ZERO_VALUE.match(msg.strip()), f"'{msg}' should be zero-value"

    @pytest.mark.parametrize("msg", [
        "what's happening with polymarket today",
        "should we investigate the new framework",
        "can you research the latest api changes",
        "there's a bug in the deploy pipeline",
        "how does this compare to the competitor",
    ])
    def test_value_messages_not_zero(self, msg):
        assert not _ZERO_VALUE.match(msg.strip()), f"'{msg}' should have value"

    @pytest.mark.parametrize("msg,expected_type", [
        ("check polymarket for election odds", "market_research"),
        ("should we switch to a different framework", "strategy_research"),
        ("research the latest rust async patterns", "explicit_research"),
        ("the deploy pipeline is broken", "incident_research"),
        ("supabase is a competitor to our stack", "competitive_research"),
    ])
    def test_value_pattern_matching(self, msg, expected_type):
        lowered = msg.lower()
        matched = False
        for pattern, signal_type in _VALUE_PATTERNS:
            if re.search(pattern, lowered):
                if signal_type == expected_type:
                    matched = True
                    break
        assert matched, f"'{msg}' should match signal type '{expected_type}'"


class TestChatEngine:
    def test_get_session_creates_new(self):
        engine = ChatEngine()
        s = engine.get_session("user-1")
        assert s.session_id == "user-1"
        assert len(s.messages) == 0

    def test_get_session_returns_same(self):
        engine = ChatEngine()
        s1 = engine.get_session("user-1")
        s1.add("operator", "first")
        s2 = engine.get_session("user-1")
        assert len(s2.messages) == 1

    def test_prune_stale_sessions(self):
        engine = ChatEngine()
        s = engine.get_session("old-session")
        s.last_active = 0  # epoch = very old
        engine.get_session("new-session")
        removed = engine.prune_stale_sessions(max_age_sec=10)
        assert removed == 1
        assert "old-session" not in engine._sessions
        assert "new-session" in engine._sessions


class TestCostCascade:
    """Verify LLM routing order: boost node → Gemini Flash → Gemini Pro."""

    @pytest.mark.asyncio
    async def test_boost_node_tried_first(self):
        engine = ChatEngine()
        with patch.object(engine, '_try_boost_node', new_callable=AsyncMock, return_value="boost reply") as mock_boost:
            reply = await engine._route_llm("test prompt")
            assert reply == "boost reply"
            mock_boost.assert_called_once()

    @pytest.mark.asyncio
    async def test_falls_back_to_gemini_when_no_boost(self):
        engine = ChatEngine()
        mock_llm = MagicMock()
        mock_llm.generate.return_value = "gemini reply"
        engine._llm = mock_llm
        with patch.object(engine, '_try_boost_node', new_callable=AsyncMock, return_value=None):
            reply = await engine._route_llm("test prompt")
            assert reply == "gemini reply"

    @pytest.mark.asyncio
    async def test_returns_empty_when_all_fail(self):
        engine = ChatEngine()
        mock_llm = MagicMock()
        mock_llm.generate.return_value = ""
        engine._llm = mock_llm
        with patch.object(engine, '_try_boost_node', new_callable=AsyncMock, return_value=None):
            reply = await engine._route_llm("test prompt")
            assert reply == ""
