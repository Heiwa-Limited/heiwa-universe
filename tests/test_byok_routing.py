import pytest
import uuid
import time
from unittest.mock import MagicMock, patch
from heiwa_protocol.routing import BrokerRouteRequest, BrokerRouteResult
from apps.heiwa_hub.mcp_server import create_task

@pytest.mark.asyncio
async def test_mcp_task_ingress_carries_owner_id():
    # Mocking the dependencies of create_task
    with patch("apps.heiwa_hub.mcp_server._validate_auth_token", return_value="fake-token"), \
         patch("apps.heiwa_hub.mcp_server._ws_client_claims", return_value={"owner_id": "user-123", "principal_id": "user-123"}), \
         patch("apps.heiwa_hub.mcp_server.resolve_identity_context", return_value={"owner_id": "user-123", "principal_id": "user-123"}), \
         patch("apps.heiwa_hub.mcp_server.enrichment.enrich") as mock_enrich, \
         patch("apps.heiwa_hub.mcp_server.claw_gateway.resolve") as mock_resolve, \
         patch("apps.heiwa_hub.mcp_server.cells.recommend", return_value={"cell": {}}), \
         patch("apps.heiwa_hub.mcp_server.get_bus") as mock_bus, \
         patch("apps.heiwa_hub.mcp_server._snapshot_task"), \
         patch("apps.heiwa_hub.mcp_server.db.create_mission"), \
         patch("apps.heiwa_hub.mcp_server.db.append_mission_step"), \
         patch("apps.heiwa_hub.mcp_server._append_wire_event"):
        
        mock_enrich.return_value = MagicMock(spec=BrokerRouteResult)
        mock_enrich.return_value.to_dict.return_value = {}
        mock_resolve.return_value = MagicMock()
        mock_resolve.return_value.to_dict.return_value = {}
        
        mock_publish = MagicMock()
        mock_bus.return_value.publish = mock_publish
        
        # Simulate a task request
        req = MagicMock()
        req.raw_text = "test task"
        req.sender_id = "cli"
        req.source_surface = "cli"
        req.privacy_level = None
        req.task_id = None
        req.session_id = None
        req.battlefield_id = None
        
        await create_task(req, authorization="Bearer fake-token")
        
        # Verify enrichment was called with the correct owner_id
        args, _ = mock_enrich.call_args
        assert args[0].owner_id == "user-123"
        
        # Verify bus publication carries owner_id
        _, publish_kwargs = mock_publish.call_args
        assert publish_kwargs["data"]["owner_id"] == "user-123"
