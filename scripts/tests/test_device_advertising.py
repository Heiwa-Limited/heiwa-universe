import pytest
from unittest.mock import MagicMock, patch
from heiwa_sdk.spacetimedb import SpacetimeDB
from heiwa_identity.node import gather_device_capabilities

def test_gather_device_capabilities_basic():
    caps = gather_device_capabilities()
    assert "vram_mb" in caps
    assert "locality" in caps
    assert "trust_tier" in caps
    assert isinstance(caps["provider_keys"], list)
    assert isinstance(caps["model_inventory"], list)

@patch("heiwa_sdk.spacetimedb.SpacetimeDB.call")
def test_upsert_node_heartbeat_binding(mock_call):
    db = SpacetimeDB(server="local", db_identity="test")
    db.upsert_node_heartbeat(
        node_id="test-node",
        vram_mb=8192,
        locality="local",
        trust_tier=10,
        provider_keys=["openai"],
        model_inventory=["ollama/llama3"]
    )
    
    args = mock_call.call_args[0]
    assert args[0] == "upsert_node_heartbeat"
    assert len(args) == 14 # name + 13 args
    assert args[9] == 8192
    assert args[10] == "local"
    assert args[11] == 10
    assert '"openai"' in args[12]
    assert '"ollama/llama3"' in args[13]

@patch("heiwa_sdk.spacetimedb.SpacetimeDB.call")
def test_upsert_session_binding(mock_call):
    db = SpacetimeDB(server="local", db_identity="test")
    db.upsert_session(
        session_id="sess-123",
        node_id="node-1",
        session_type="worker",
        metadata={"version": "1.0"}
    )
    
    args = mock_call.call_args[0]
    assert args[0] == "upsert_session"
    assert args[1] == "sess-123"
    assert args[2] is None
    assert args[3] == "node-1"
    assert args[4] == "worker"
    assert '"1.0"' in args[6]

@patch("heiwa_sdk.spacetimedb.SpacetimeDB.call")
def test_close_session_binding(mock_call):
    db = SpacetimeDB(server="local", db_identity="test")
    db.close_session("sess-123")
    
    args = mock_call.call_args[0]
    assert args[0] == "close_session"
    assert args[1] == "sess-123"
