import importlib
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


def _load_mcp_server():
    sys.modules.pop("apps.heiwa_hub.mcp_server", None)
    return importlib.import_module("apps.heiwa_hub.mcp_server")


def test_hub_health_allows_phase1_web_origins(monkeypatch):
    with tempfile.TemporaryDirectory() as tmpdir:
        monkeypatch.setenv("HEIWA_STATE_BACKEND", "compatibility_sqlite")
        monkeypatch.setenv("DATABASE_PATH", str(Path(tmpdir) / "hub.db"))

        mcp_server = _load_mcp_server()
        client = TestClient(mcp_server.app)

        for origin in ("https://heiwa.ltd", "https://app.heiwa.ltd"):
            response = client.get("/health", headers={"Origin": origin})

            assert response.status_code == 200
            assert response.headers["access-control-allow-origin"] == origin
