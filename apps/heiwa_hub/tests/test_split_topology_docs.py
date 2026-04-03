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

    assert "app.heiwa.ltd" in text
    assert "heiwa-cloud-hq" in text
    assert "maincloud.spacetimedb.com" in text
    assert "not part of the supported public surface" in text


def test_public_index_points_get_started_at_live_oauth_entry():
    index_text = (ROOT / "apps" / "heiwa_web" / "clients" / "web" / "index.html").read_text(encoding="utf-8")

    assert 'href="https://api.heiwa.ltd/auth/discord"' in index_text


def test_domain_manifest_includes_app_host_and_external_state():
    data = json.loads(DOMAIN_MANIFEST.read_text(encoding="utf-8"))
    hosts = {entry["host"] for entry in data["domains"]}

    assert hosts == {
        "heiwa.ltd",
        "app.heiwa.ltd",
        "status.heiwa.ltd",
        "api.heiwa.ltd",
        "docs.heiwa.ltd",
    }
    assert data["platform"]["state_ledger"] == "spacetimedb_maincloud"
    assert data["platform"]["state_endpoint"] == "maincloud.spacetimedb.com"

    by_host = {entry["host"]: entry for entry in data["domains"]}
    assert "Cloudflare Pages" in by_host["app.heiwa.ltd"]["target"]
    assert "heiwa-core" in by_host["api.heiwa.ltd"]["target"]


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
            {"host": "trade.heiwa.ltd"},
            {"host": "docs.heiwa.ltd"},
        ],
    }
    path = tmp_path / "domains.bootstrap.json"
    path.write_text(json.dumps(stale_manifest), encoding="utf-8")

    problems = module.check_domain_manifest(path)

    assert any("app.heiwa.ltd" in problem for problem in problems)
    assert any("trade.heiwa.ltd" in problem for problem in problems)
    assert any("maincloud.spacetimedb.com" in problem for problem in problems)


def test_security_doc_describes_public_trust_boundaries():
    text = (ROOT / "docs" / "security.md").read_text(encoding="utf-8")

    assert "app.heiwa.ltd" in text
    assert "api.heiwa.ltd" in text
    assert "BYOK credentials" in text
    assert "operator auth" in text
