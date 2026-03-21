"""Tests for the standalone Heiwa Trading service app."""

from pathlib import Path
import sys

from fastapi.testclient import TestClient


ROOT = Path(__file__).resolve().parents[3]
SRC = ROOT / "apps" / "heiwa_trading" / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from heiwa_trading.app import app


client = TestClient(app)


def test_root_serves_trading_cockpit():
    response = client.get("/")
    assert response.status_code == 200
    assert "text/html" in response.headers["content-type"]
    assert "Heiwa Trading" in response.text
    assert "/trading/cockpit.css" in response.text


def test_health_reports_trading_service():
    response = client.get("/health")
    assert response.status_code == 200
    payload = response.json()
    assert payload["status"] == "alive"
    assert payload["service"] == "heiwa-trading"
