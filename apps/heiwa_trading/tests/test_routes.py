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


def test_cockpit_rejects_without_auth(client):
    response = client.get("/trading/cockpit")
    assert response.status_code == 401


def test_cockpit_page_returns_html(client):
    response = client.get("/trading/cockpit", headers=AUTH)
    assert response.status_code == 200
    assert "text/html" in response.headers["content-type"]
    assert "Heiwa Trading" in response.text


def test_cockpit_css_returns_css(client):
    response = client.get("/trading/cockpit.css", headers=AUTH)
    assert response.status_code == 200
    assert "text/css" in response.headers["content-type"]


def test_cockpit_js_returns_js(client):
    response = client.get("/trading/cockpit.js", headers=AUTH)
    assert response.status_code == 200
    assert "javascript" in response.headers["content-type"]


def test_trading_state_returns_json(client):
    response = client.get("/trading/api/state", headers=AUTH)
    assert response.status_code == 200
    data = response.json()
    assert isinstance(data, dict)


def test_cockpit_no_mac_agent_branding(client):
    """Verify all Mac Agent branding has been removed."""
    response = client.get("/trading/cockpit", headers=AUTH)
    text = response.text.lower()
    assert "mac agent" not in text
    assert "mac-agent" not in text


def test_cockpit_css_js_paths_correct(client):
    """Verify CSS and JS references use /trading/ prefix."""
    response = client.get("/trading/cockpit", headers=AUTH)
    assert "/trading/cockpit.css" in response.text
    assert "/trading/cockpit.js" in response.text
