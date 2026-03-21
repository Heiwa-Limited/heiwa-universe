import importlib.util
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
DOMAIN_PLAN = ROOT / "config" / "swarm" / "domain_plan.md"
DOMAIN_MANIFEST = ROOT / "apps" / "heiwa_web" / "clients" / "web" / "assets" / "domains.bootstrap.json"
STATIC_SURFACE_GUARD = ROOT / "apps" / "heiwa_web" / "scripts" / "check_static_surface.py"


def _load_guard_module():
    spec = importlib.util.spec_from_file_location("check_static_surface", STATIC_SURFACE_GUARD)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def test_domain_plan_describes_split_service_topology():
    text = DOMAIN_PLAN.read_text(encoding="utf-8")

    assert "trade.heiwa.ltd" in text
    assert "heiwa-cloud-hq" in text
    assert "heiwa-trading" in text
    assert "maincloud.spacetimedb.com" in text


def test_domain_manifest_includes_trade_host_and_external_state():
    data = json.loads(DOMAIN_MANIFEST.read_text(encoding="utf-8"))
    hosts = {entry["host"] for entry in data["domains"]}

    assert hosts == {
        "heiwa.ltd",
        "status.heiwa.ltd",
        "api.heiwa.ltd",
        "trade.heiwa.ltd",
        "docs.heiwa.ltd",
    }
    assert data["platform"]["state_ledger"] == "spacetimedb_maincloud"
    assert data["platform"]["state_endpoint"] == "maincloud.spacetimedb.com"

    by_host = {entry["host"]: entry for entry in data["domains"]}
    assert "heiwa-cloud-hq" in by_host["api.heiwa.ltd"]["target"]
    assert "heiwa-trading" in by_host["trade.heiwa.ltd"]["target"]


def test_static_surface_guard_rejects_stale_four_host_manifest(tmp_path: Path):
    module = _load_guard_module()
    stale_manifest = {
        "platform": {
            "dns": "cloudflare",
            "public_web": "cloudflare_pages",
            "control_plane": "railway",
            "edge_security": "cloudflare_waf",
        },
        "domains": [
            {"host": "heiwa.ltd"},
            {"host": "status.heiwa.ltd"},
            {"host": "api.heiwa.ltd"},
            {"host": "docs.heiwa.ltd"},
        ],
    }
    path = tmp_path / "domains.bootstrap.json"
    path.write_text(json.dumps(stale_manifest), encoding="utf-8")

    problems = module.check_domain_manifest(path)

    assert any("trade.heiwa.ltd" in problem for problem in problems)
    assert any("maincloud.spacetimedb.com" in problem for problem in problems)
