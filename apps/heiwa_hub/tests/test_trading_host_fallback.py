"""Regression test for the retired trade.heiwa.ltd root fallback on the hub."""

import importlib
import os
import sys
import tempfile
from pathlib import Path

from fastapi.testclient import TestClient


ROOT = Path(__file__).resolve().parents[3]
for pkg in ("heiwa_sdk", "heiwa_protocol", "heiwa_identity", "heiwa_ui"):
    path = ROOT / "packages" / pkg
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))
if str(ROOT / "apps") not in sys.path:
    sys.path.insert(0, str(ROOT / "apps"))


def test_hub_root_does_not_special_case_trade_host(monkeypatch):
    with tempfile.TemporaryDirectory() as tmpdir:
        monkeypatch.setenv("HEIWA_STATE_BACKEND", "compatibility_sqlite")
        monkeypatch.setenv("DATABASE_PATH", str(Path(tmpdir) / "hub.db"))

        sys.modules.pop("apps.heiwa_hub.mcp_server", None)
        mcp_server = importlib.import_module("apps.heiwa_hub.mcp_server")
        client = TestClient(mcp_server.app)

        default_response = client.get("/", headers={"host": "heiwa.ltd"})
        trade_response = client.get("/", headers={"host": "trade.heiwa.ltd"})

        assert default_response.status_code == 200
        assert trade_response.status_code == 200
        assert trade_response.text == default_response.text
        assert "Heiwa Trading Cockpit" not in trade_response.text


def test_hub_no_longer_serves_trading_routes(monkeypatch):
    with tempfile.TemporaryDirectory() as tmpdir:
        monkeypatch.setenv("HEIWA_STATE_BACKEND", "compatibility_sqlite")
        monkeypatch.setenv("DATABASE_PATH", str(Path(tmpdir) / "hub.db"))

        sys.modules.pop("apps.heiwa_hub.mcp_server", None)
        mcp_server = importlib.import_module("apps.heiwa_hub.mcp_server")
        client = TestClient(mcp_server.app)

        response = client.get("/trading/cockpit")

        assert response.status_code == 404


def test_hub_server_source_no_longer_imports_trading_router():
    source = (ROOT / "apps" / "heiwa_hub" / "mcp_server.py").read_text(encoding="utf-8")

    assert "from heiwa_trading.routes import router as trading_router" not in source
    assert "app.include_router(trading_router)" not in source


def test_hub_runtime_bundle_no_longer_packages_trading_app():
    dockerfile = (ROOT / "apps" / "heiwa_hub" / "Dockerfile").read_text(encoding="utf-8")
    start_script = (ROOT / "apps" / "heiwa_hub" / "start.sh").read_text(encoding="utf-8")

    assert "/app/apps/heiwa_trading/src" not in dockerfile
    assert "COPY apps/heiwa_trading/" not in dockerfile
    assert "/app/apps/heiwa_trading/src" not in start_script
