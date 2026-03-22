from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
START_SCRIPT = ROOT / "apps" / "heiwa_hub" / "start.sh"
DOCKERFILE = ROOT / "apps" / "heiwa_hub" / "Dockerfile"
SPACETIME_MANIFEST = ROOT / "apps" / "heiwa_hub" / "spacetime.json"


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def test_cloud_hq_publish_targets_manifest_directory():
    script = read_text(START_SCRIPT)

    assert SPACETIME_MANIFEST.exists()
    assert 'STDB_PROJECT_DIR="apps/heiwa_hub"' in script
    assert 'STDB_MANIFEST_PATH="$STDB_PROJECT_DIR/spacetime.json"' in script
    assert '(cd "$STDB_PROJECT_DIR" && spacetime publish --server "$STDB_SERVER" "$STDB_IDENTITY")' in script


def test_cloud_hq_boot_does_not_swallow_stdb_publish_failures():
    script = read_text(START_SCRIPT)

    assert 'STDB publish failed, continuing' not in script
    assert 'exit 1' in script


def test_cloud_hq_image_installs_control_plane_clis():
    dockerfile = read_text(DOCKERFILE)

    assert "@railway/cli" in dockerfile
    assert "wrangler" in dockerfile
    assert " gh " in dockerfile or "\n    gh \\\n" in dockerfile or "install -y gh" in dockerfile


def test_cloud_hq_bootstrap_configures_noninteractive_control_plane_auth():
    script = read_text(START_SCRIPT)

    assert "GH_TOKEN" in script
    assert "GITHUB_TOKEN" in script
    assert "RAILWAY_TOKEN" in script
    assert "CLOUDFLARE_API_TOKEN" in script
    assert "SPACETIMEDB_TOKEN" in script or "STDB_AUTH_TOKEN" in script
    assert "gh auth status" in script
    assert "railway whoami" in script
    assert "wrangler whoami" in script
    assert "spacetime login --token" in script
