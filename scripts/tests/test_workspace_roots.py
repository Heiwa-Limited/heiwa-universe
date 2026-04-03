from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]


def _load_module(module_name: str, path: Path):
    spec = importlib.util.spec_from_file_location(module_name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


def _make_repo(path: Path) -> Path:
    (path / "apps").mkdir(parents=True, exist_ok=True)
    (path / "packages").mkdir(parents=True, exist_ok=True)
    return path


def test_sdk_config_prefers_workspace_root_env(monkeypatch, tmp_path):
    workspace_root = _make_repo(tmp_path / "heiwa-universe")
    legacy_root = _make_repo(tmp_path / "heiwa")

    monkeypatch.setenv("HEIWA_WORKSPACE_ROOT", str(workspace_root))
    monkeypatch.setenv("HEIWA_ROOT", str(legacy_root))

    module = _load_module(
        "test_heiwa_sdk_config_workspace",
        REPO_ROOT / "packages/heiwa_sdk/heiwa_sdk/config.py",
    )

    assert module.MONOREPO_ROOT == workspace_root


def test_heiwa_360_check_prefers_home_heiwa_universe(monkeypatch, tmp_path):
    home = tmp_path / "home"
    workspace_root = _make_repo(home / "heiwa-universe")
    _make_repo(home / "heiwa")
    outside = tmp_path / "outside"
    outside.mkdir()

    monkeypatch.delenv("HEIWA_WORKSPACE_ROOT", raising=False)
    monkeypatch.delenv("HEIWA_ROOT", raising=False)
    monkeypatch.delenv("HEIWA_ROOT_DIR", raising=False)
    monkeypatch.setenv("HOME", str(home))

    module = _load_module(
        "test_heiwa_360_workspace",
        REPO_ROOT / "apps/heiwa_cli/scripts/ops/heiwa_360_check.py",
    )

    assert module.find_monorepo_root(outside) == workspace_root


def test_identity_root_prefers_home_heiwa_universe(monkeypatch, tmp_path):
    home = tmp_path / "home"
    workspace_root = _make_repo(home / "heiwa-universe")
    _make_repo(home / "heiwa")
    outside = tmp_path / "outside" / "node.py"
    outside.parent.mkdir(parents=True)
    outside.write_text("# placeholder\n", encoding="utf-8")

    monkeypatch.delenv("HEIWA_WORKSPACE_ROOT", raising=False)
    monkeypatch.delenv("HEIWA_ROOT", raising=False)
    monkeypatch.delenv("HEIWA_ROOT_DIR", raising=False)
    monkeypatch.setenv("HOME", str(home))

    module = _load_module(
        "test_heiwa_identity_workspace",
        REPO_ROOT / "packages/heiwa_identity/heiwa_identity/node.py",
    )

    assert module.discover_monorepo_root(outside) == workspace_root


def test_routing_root_prefers_workspace_root_env(monkeypatch, tmp_path):
    workspace_root = _make_repo(tmp_path / "heiwa-universe")
    legacy_root = _make_repo(tmp_path / "heiwa")
    outside = tmp_path / "outside" / "routing.py"
    outside.parent.mkdir(parents=True)
    outside.write_text("# placeholder\n", encoding="utf-8")

    monkeypatch.setenv("HEIWA_WORKSPACE_ROOT", str(workspace_root))
    monkeypatch.setenv("HEIWA_ROOT", str(legacy_root))

    module = _load_module(
        "test_heiwa_routing_workspace",
        REPO_ROOT / "packages/heiwa_sdk/heiwa_sdk/routing.py",
    )

    assert module.discover_monorepo_root(outside) == workspace_root


def test_isolation_root_dir_prefers_home_heiwa_universe(monkeypatch, tmp_path):
    home = tmp_path / "home"
    workspace_root = _make_repo(home / "heiwa-universe")
    _make_repo(home / "heiwa")
    outside = tmp_path / "outside" / "isolation.py"
    outside.parent.mkdir(parents=True)
    outside.write_text("# placeholder\n", encoding="utf-8")

    monkeypatch.delenv("HEIWA_WORKSPACE_ROOT", raising=False)
    monkeypatch.delenv("HEIWA_ROOT", raising=False)
    monkeypatch.delenv("HEIWA_ROOT_DIR", raising=False)
    monkeypatch.setenv("HOME", str(home))

    module = _load_module(
        "test_heiwa_isolation_workspace",
        REPO_ROOT / "packages/heiwa_sdk/heiwa_sdk/sanity/isolation.py",
    )

    assert module.resolve_root_dir(outside) == workspace_root
