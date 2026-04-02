import requests
import json
import websockets
import os
import pytest
import asyncio

HEIWA_CORE_URL = os.getenv("HEIWA_CORE_URL", "http://localhost:8080")
HEIWA_AUTH_TOKEN = os.getenv("HEIWA_AUTH_TOKEN", "test-token")

def test_health():
    response = requests.get(f"{HEIWA_CORE_URL}/health")
    assert response.status_code == 200
    assert response.json()["status"] == "ok"

def test_status():
    response = requests.get(f"{HEIWA_CORE_URL}/status")
    assert response.status_code == 200
    assert "node_id" in response.json()

def test_legacy_battlefield():
    payload = {"name": "test-battlefield"}
    response = requests.post(f"{HEIWA_CORE_URL}/battlefields", json=payload)
    assert response.status_code == 200
    assert "battlefield_id" in response.json()

def test_legacy_task():
    payload = {"input": "test task"}
    response = requests.post(f"{HEIWA_CORE_URL}/tasks", json=payload)
    assert response.status_code == 200
    assert "task_id" in response.json()

@pytest.mark.asyncio
async def test_ws_auth():
    ws_url = HEIWA_CORE_URL.replace("http", "ws") + "/ws"
    async with websockets.connect(ws_url) as websocket:
        auth_msg = {
            "type": "auth",
            "token": HEIWA_AUTH_TOKEN
        }
        await websocket.send(json.dumps(auth_msg))
        response = await websocket.recv()
        result = json.loads(response)
        
        assert result["type"] == "auth_ok"

@pytest.mark.asyncio
async def test_route_preview():
    ws_url = HEIWA_CORE_URL.replace("http", "ws") + "/ws"
    async with websockets.connect(ws_url) as websocket:
        # Auth first
        auth_msg = {"type": "auth", "token": HEIWA_AUTH_TOKEN}
        await websocket.send(json.dumps(auth_msg))
        await websocket.recv()

        # Route preview action
        action_msg = {
            "type": "action",
            "action": "route.preview",
            "request_id": "req-123",
            "payload": {
                "intent": "build",
                "risk": "low",
                "raw_text": "write a python script",
                "privacy": "standard",
                "runtime": "railway",
                "available_vram_mb": 0,
                "required_context_tokens": 1000
            }
        }
        await websocket.send(json.dumps(action_msg))
        response = await websocket.recv()
        result = json.loads(response)
        
        assert result["type"] == "result"
        assert result["request_id"] == "req-123"
        assert "target_tier" in result["payload"]
        assert "task_id" in result["payload"]

if __name__ == "__main__":
    # Manual execution helper
    import sys
    pytest.main([__file__] + sys.argv[1:])
