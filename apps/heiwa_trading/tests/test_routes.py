"""Integration tests for heiwa_trading FastAPI routes."""
import pytest
from fastapi.testclient import TestClient
from fastapi import FastAPI

from heiwa_trading.routes import router


@pytest.fixture
def client():
    app = FastAPI()
    app.include_router(router)
    return TestClient(app)


def test_cockpit_page_returns_html(client):
    response = client.get("/trading/cockpit")
    assert response.status_code == 200
    assert "text/html" in response.headers["content-type"]
    assert "Heiwa Trading" in response.text


def test_cockpit_css_returns_css(client):
    response = client.get("/trading/cockpit.css")
    assert response.status_code == 200
    assert "text/css" in response.headers["content-type"]


def test_cockpit_js_returns_js(client):
    response = client.get("/trading/cockpit.js")
    assert response.status_code == 200
    assert "javascript" in response.headers["content-type"]


def test_trading_state_returns_json(client):
    response = client.get("/trading/api/state")
    assert response.status_code == 200
    data = response.json()
    assert isinstance(data, dict)


def test_cockpit_no_mac_agent_branding(client):
    """Verify all Mac Agent branding has been removed."""
    response = client.get("/trading/cockpit")
    text = response.text.lower()
    assert "mac agent" not in text
    assert "mac-agent" not in text
