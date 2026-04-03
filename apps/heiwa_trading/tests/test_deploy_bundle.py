from pathlib import Path

from heiwa_trading.cockpit import resolve_agent_root


ROOT = Path(__file__).resolve().parents[1]


def test_railway_manifest_uses_local_paths():
    text = (ROOT / "railway.toml").read_text(encoding="utf-8")

    assert 'dockerfilePath = "Dockerfile"' in text
    assert 'startCommand = "bash start.sh"' in text
    assert "apps/heiwa_trading/" not in text


def test_dockerfile_is_self_contained_for_path_root_deploys():
    text = (ROOT / "Dockerfile").read_text(encoding="utf-8")

    assert "COPY requirements.txt ." in text
    assert "COPY src/ /app/src/" in text
    assert "COPY start.sh /app/start.sh" in text
    assert "/app/apps/heiwa_trading" not in text


def test_start_script_boots_from_local_app_root():
    text = (ROOT / "start.sh").read_text(encoding="utf-8")

    assert 'cd /app || exit 1' in text
    assert '/app/src' in text
    assert '/app/apps/heiwa_trading' not in text


def test_resolve_agent_root_falls_back_for_standalone_layout():
    assert resolve_agent_root(Path("/app")) == Path("/app")
    assert resolve_agent_root(Path("/Users/dmcgregsauce/heiwa/apps/heiwa_trading")) == Path("/Users/dmcgregsauce/heiwa-universe")
