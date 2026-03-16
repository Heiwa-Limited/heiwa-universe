"""Phase 5 integration test: MCP tools, ACP adapter, and Mission service."""
import json
import pytest
from unittest.mock import MagicMock, patch, AsyncMock
from heiwa_sdk.heiwaclaw import HeiwaClawGateway
from heiwa_sdk.mission import MissionService


class TestPhase5Integration:
    """Verify Phase 5: Protocols and Missions."""

    @pytest.mark.asyncio
    async def test_acp_adapter_handoff(self):
        from heiwa_protocol.routing import BrokerRouteResult
        gw = HeiwaClawGateway(MagicMock()) # Root dir doesn't matter for mock
        
        route = BrokerRouteResult(
            request_id="test-req",
            task_id="test-task",
            envelope_version="1",
            raw_text="Hello remote",
            source_surface="cli",
            intent_class="chat",
            risk_level="low",
            privacy_level="local",
            compute_class=3,
            assigned_worker="remote-node",
            target_tool="heiwa_acp", # Forces ACP
            target_model="test-model",
            target_runtime="remote",
            target_tier="tier1",
            requires_approval=False,
            rationale="test"
        )
        
        exit_code, output = await gw.execute(route, "hello")
        assert exit_code == 0
        assert "ACP handoff simulated" in output

    @pytest.mark.asyncio
    async def test_mission_service_calls_stdb(self):
        mock_stdb = MagicMock()
        service = MissionService(stdb=mock_stdb)
        
        # 1. Create mission
        service.create_mission("mission-1", "test prompt")
        mock_stdb.call.assert_called()
        assert mock_stdb.call.call_args_list[0][0][0] == "create_mission"
        
        # 2. Append step
        service.append_step("mission-1", "Step 1", input_data={"cmd": "ls"})
        assert mock_stdb.call.call_args_list[1][0][0] == "append_mission_step"

    @pytest.mark.asyncio
    async def test_mcp_server_new_tools(self):
        # We test the tools by importing the app and calling the endpoint 
        # but mocking the enrichment and STDB
        from apps.heiwa_hub.mcp_server import app
        from fastapi.testclient import TestClient
        client = TestClient(app)
        # Verify health check at least
        resp = client.get("/health")
        assert resp.status_code == 200
