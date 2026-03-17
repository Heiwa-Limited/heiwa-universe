"""Tests for HeiwaAgent memory loop and compression."""
import pytest
from unittest.mock import MagicMock, AsyncMock, patch


class TestHeiwaAgentMemory:
    """Test memory loop integration in HeiwaAgent."""

    def _make_agent(self):
        with patch("heiwa_hub.agents.heiwa_agent.RepoAuditor"):
            from heiwa_hub.agents.heiwa_agent import HeiwaAgent
            agent = HeiwaAgent()
            agent.db = MagicMock()
            agent.db.stdb = MagicMock()
            agent._llm = MagicMock()
            return agent

    def test_agent_has_memory(self):
        agent = self._make_agent()
        assert agent.agent_memory is not None

    # ── STORE (steps 2 & 6): operator and agent messages persisted ──

    def test_store_operator_message(self):
        agent = self._make_agent()
        agent.agent_memory.store_message = MagicMock(return_value=True)

        agent._store_operator_message("hello from operator", source="discord_dm")

        agent.agent_memory.store_message.assert_called_once_with(
            role="operator", content="hello from operator", source="discord_dm"
        )

    def test_store_agent_response(self):
        agent = self._make_agent()
        agent.agent_memory.store_message = MagicMock(return_value=True)

        agent._store_agent_response("I see the deploy is running")

        agent.agent_memory.store_message.assert_called_once_with(
            role="agent", content="I see the deploy is running", source="system"
        )

    # ── RECEIVE (step 1): event handlers store operator input ──

    @pytest.mark.asyncio
    async def test_on_task_ingress_stores_operator_message(self):
        agent = self._make_agent()
        agent._store_operator_message = MagicMock()
        agent.speak = AsyncMock()

        await agent._on_task_ingress({
            "data": {
                "task_id": "t1",
                "raw_text": "deploy the app",
                "intent_class": "build",
                "source": "discord",
            }
        })

        agent._store_operator_message.assert_called_once()
        call_args = agent._store_operator_message.call_args
        assert "deploy the app" in call_args[0][0]

    # ── COMPRESSION ──

    @pytest.mark.asyncio
    async def test_compression_triggered_when_over_budget(self):
        agent = self._make_agent()
        agent.agent_memory.load_context_window = MagicMock(return_value={
            "messages": [{"content": "x" * 128000, "timestamp": 100}],
            "focuses": [],
            "summaries": [],
        })
        agent.agent_memory.needs_compression = MagicMock(return_value=True)
        agent._run_rolling_compression = AsyncMock()

        await agent._maybe_compress()

        agent._run_rolling_compression.assert_called_once()

    @pytest.mark.asyncio
    async def test_no_compression_when_under_budget(self):
        agent = self._make_agent()
        agent.agent_memory.load_context_window = MagicMock(return_value={
            "messages": [{"content": "short"}],
            "focuses": [],
            "summaries": [],
        })
        agent.agent_memory.needs_compression = MagicMock(return_value=False)
        agent._run_rolling_compression = AsyncMock()

        await agent._maybe_compress()

        agent._run_rolling_compression.assert_not_called()

    # ── BOOT HYDRATION ──

    def test_boot_hydration_loads_context(self):
        agent = self._make_agent()
        agent.agent_memory.load_context_window = MagicMock(return_value={
            "messages": [
                {"role": "operator", "content": "last thing we discussed"},
            ],
            "focuses": [{"topic": "Railway deploy"}],
            "summaries": [{"content": "Yesterday we worked on..."}],
        })

        ctx = agent._hydrate_boot_context()
        assert len(ctx["messages"]) == 1
        assert len(ctx["focuses"]) == 1
        assert len(ctx["summaries"]) == 1

    # ── REASON (step 5): complexity detection for cascade ──

    def test_complexity_detection_routes_to_cascade(self):
        agent = self._make_agent()
        agent.agent_memory.detect_complexity = MagicMock(return_value=True)
        assert agent._should_cascade("redesign the architecture and deploy strategy") is True

    def test_simple_message_stays_on_flash(self):
        agent = self._make_agent()
        agent.agent_memory.detect_complexity = MagicMock(return_value=False)
        assert agent._should_cascade("status?") is False

    # ── FOCUS (step 7): topic tracking ──

    def test_update_focus_creates_entry(self):
        agent = self._make_agent()
        agent.agent_memory.upsert_focus = MagicMock(return_value="focus-123")
        agent.agent_memory.stdb.get_active_focuses = MagicMock(return_value=[])

        agent._update_focus("Railway deployment", {"task_id": "t1"})

        agent.agent_memory.upsert_focus.assert_called_once()
        call_kw = agent.agent_memory.upsert_focus.call_args
        assert call_kw[1]["topic"] == "Railway deployment"

    def test_update_focus_updates_existing(self):
        agent = self._make_agent()
        agent.agent_memory.upsert_focus = MagicMock(return_value="focus-existing")
        agent.agent_memory.stdb.get_active_focuses = MagicMock(return_value=[
            {"focus_id": "focus-existing", "topic": "Railway deployment", "priority": 3},
        ])

        agent._update_focus("Railway deployment", {"task_id": "t2"})

        agent.agent_memory.upsert_focus.assert_called_once()
        # Should reuse existing focus_id
        call_kw = agent.agent_memory.upsert_focus.call_args
        assert call_kw[1]["focus_id"] == "focus-existing"


class TestHealthCheck:
    """Test STDB-aware health check."""

    @pytest.mark.asyncio
    async def test_health_returns_503_when_stdb_down(self):
        with patch("apps.heiwa_hub.mcp_server.db") as mock_db:
            mock_db.state_backend = "spacetimedb"
            mock_db.stdb = MagicMock()
            # Simulate failure
            mock_db.stdb.query.side_effect = Exception("connection refused")

            from apps.heiwa_hub.mcp_server import _check_stdb_health
            result = await _check_stdb_health()
            assert result is False

    @pytest.mark.asyncio
    async def test_health_returns_200_when_stdb_up(self):
        with patch("apps.heiwa_hub.mcp_server.db") as mock_db:
            mock_db.state_backend = "spacetimedb"
            mock_db.stdb = MagicMock()
            mock_db.stdb.query.return_value = [{"model_id": "test"}]

            from apps.heiwa_hub.mcp_server import _check_stdb_health
            result = await _check_stdb_health()
            assert result is True
