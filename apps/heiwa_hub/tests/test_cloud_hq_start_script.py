from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
START_SCRIPT = ROOT / "apps" / "heiwa_hub" / "start.sh"
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
