import requests
import json
import websocket
import os

HEIWA_CORE_URL = os.getenv("HEIWA_CORE_URL", "http://localhost:8080")
HEIWA_AUTH_TOKEN = os.getenv("HEIWA_AUTH_TOKEN", "")

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
    assert response.json()["status"] == "ok"

def test_legacy_task():
    payload = {"input": "test task"}
    response = requests.post(f"{HEIWA_CORE_URL}/tasks", json=payload)
    assert response.status_code == 200
    assert response.json()["status"] == "ok"

def test_ws_auth():
    ws_url = HEIWA_CORE_URL.replace("http", "ws") + "/ws"
    ws = websocket.create_connection(ws_url)
    
    auth_msg = {
        "type": "auth",
        "token": HEIWA_AUTH_TOKEN
    }
    ws.send(json.dumps(auth_msg))
    result = json.loads(ws.recv())
    
    assert result["type"] == "auth_ok"
    ws.close()

if __name__ == "__main__":
    # This is a manual test script
    try:
        test_health()
        print("Health check passed")
        test_status()
        print("Status check passed")
        test_legacy_battlefield()
        print("Legacy battlefield check passed")
        test_legacy_task()
        print("Legacy task check passed")
        print("All legacy compatibility checks passed (HTTP)")
    except Exception as e:
        print(f"Tests failed: {e}")
