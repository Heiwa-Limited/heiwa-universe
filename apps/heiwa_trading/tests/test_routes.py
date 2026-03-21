"""Integration tests for heiwa_trading FastAPI routes."""
import os
import pytest
from fastapi.testclient import TestClient
from fastapi import FastAPI

from heiwa_trading.routes import router

TEST_TOKEN = "test-token-for-ci"


@pytest.fixture(autouse=True)
def _set_auth_token(monkeypatch):
    monkeypatch.setenv("HEIWA_AUTH_TOKEN", TEST_TOKEN)


@pytest.fixture
def client():
    app = FastAPI()
    app.include_router(router)
    return TestClient(app)


AUTH = {"Authorization": f"Bearer {TEST_TOKEN}"}


def test_cockpit_page_public(client):
    """Static HTML is public (auth handled client-side)."""
    response = client.get("/trading/cockpit")
    assert response.status_code == 200
    assert "text/html" in response.headers["content-type"]
    assert "Heiwa Trading" in response.text


def test_cockpit_css_public(client):
    response = client.get("/trading/cockpit.css")
    assert response.status_code == 200
    assert "text/css" in response.headers["content-type"]


def test_cockpit_js_public(client):
    response = client.get("/trading/cockpit.js")
    assert response.status_code == 200
    assert "javascript" in response.headers["content-type"]


def test_api_state_rejects_without_auth(client):
    response = client.get("/trading/api/state")
    assert response.status_code == 401


def test_api_state_returns_json(client):
    response = client.get("/trading/api/state", headers=AUTH)
    assert response.status_code == 200
    data = response.json()
    assert isinstance(data, dict)


def test_api_settings_returns_json(client):
    response = client.get("/trading/api/settings", headers=AUTH)
    assert response.status_code == 200
    data = response.json()
    assert isinstance(data, dict)
    assert "visible_panels" in data
    assert "density" in data


def test_api_auth_validates_token(client):
    """Token validation endpoint for JS login gate."""
    # Clear rate limiter state between tests
    from heiwa_trading.routes import _auth_attempts
    _auth_attempts.clear()

    response = client.post(
        "/trading/api/auth",
        json={"token": TEST_TOKEN},
    )
    assert response.status_code == 200

    response = client.post(
        "/trading/api/auth",
        json={"token": "wrong-token"},
    )
    assert response.status_code == 403


def test_api_auth_rate_limiting(client):
    """Auth endpoint rate-limits brute-force attempts."""
    from heiwa_trading.routes import _auth_attempts, _AUTH_MAX_ATTEMPTS
    _auth_attempts.clear()

    # Exhaust the rate limit
    for _ in range(_AUTH_MAX_ATTEMPTS):
        client.post("/trading/api/auth", json={"token": "wrong"})

    # Next attempt should be rate-limited
    response = client.post("/trading/api/auth", json={"token": "wrong"})
    assert response.status_code == 429

    # Even correct token should be blocked during rate limit
    response = client.post("/trading/api/auth", json={"token": TEST_TOKEN})
    assert response.status_code == 429

    _auth_attempts.clear()


def test_cockpit_no_mac_agent_branding(client):
    """Verify all Mac Agent branding has been removed."""
    response = client.get("/trading/cockpit")
    text = response.text.lower()
    assert "mac agent" not in text
    assert "mac-agent" not in text


def test_cockpit_css_js_paths_correct(client):
    """Verify CSS and JS references use /trading/ prefix."""
    response = client.get("/trading/cockpit")
    assert "/trading/cockpit.css" in response.text
    assert "/trading/cockpit.js" in response.text


def test_sse_rejects_without_token(client):
    """SSE requires token query param."""
    response = client.get("/trading/sse")
    assert response.status_code == 401
