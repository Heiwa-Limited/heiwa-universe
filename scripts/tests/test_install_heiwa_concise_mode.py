"""Tests for the Heiwa provider-agnostic concise-mode installer."""
from __future__ import annotations

import importlib
import json
from pathlib import Path

import pytest


REPO_ROOT = Path(__file__).resolve().parents[2]
SKILL_DIR = REPO_ROOT / "packages" / "heiwa_skills" / "heiwa-concise-mode"


def load_module():
    try:
        return importlib.import_module("install_heiwa_concise_mode")
    except ModuleNotFoundError as exc:
        pytest.fail(f"install_heiwa_concise_mode module missing: {exc}")


def test_canonical_mode_files_exist():
    assert (SKILL_DIR / "SKILL.md").exists()
    assert (SKILL_DIR / "MODE.md").exists()
    assert (SKILL_DIR / "README.md").exists()


def test_install_mode_creates_expected_targets(tmp_path):
    module = load_module()

    module.install_mode(home=tmp_path, copy_mode=True)

    codex_skill = tmp_path / ".codex" / "skills" / module.MODE_ID / "SKILL.md"
    claude_skill = tmp_path / ".claude" / "skills" / module.MODE_ID / "SKILL.md"
    gemini_skill = (
        tmp_path
        / ".gemini"
        / "extensions"
        / module.MODE_ID
        / "skills"
        / module.MODE_ID
        / "SKILL.md"
    )
    gemini_manifest = (
        tmp_path / ".gemini" / "extensions" / module.MODE_ID / "gemini-extension.json"
    )
    heiwa_mode = tmp_path / ".heiwa" / "modes" / module.MODE_ID / "MODE.md"
    heiwa_manifest = tmp_path / ".heiwa" / "modes" / module.MODE_ID / "manifest.json"

    assert codex_skill.exists()
    assert claude_skill.exists()
    assert gemini_skill.exists()
    assert gemini_manifest.exists()
    assert heiwa_mode.exists()
    assert heiwa_manifest.exists()

    manifest = json.loads(gemini_manifest.read_text())
    assert manifest["name"] == module.MODE_ID
    assert manifest["contextFileName"] == "GEMINI.md"

    heiwa_data = json.loads(heiwa_manifest.read_text())
    assert heiwa_data["id"] == module.MODE_ID
    assert heiwa_data["upstream"]["repo"] == "https://github.com/JuliusBrussee/caveman"
    assert "antigravity" in heiwa_data["targets"]


def test_copy_mode_installs_real_directories(tmp_path):
    module = load_module()

    module.install_mode(home=tmp_path, copy_mode=True)

    codex_target = tmp_path / ".codex" / "skills" / module.MODE_ID
    claude_target = tmp_path / ".claude" / "skills" / module.MODE_ID
    gemini_target = tmp_path / ".gemini" / "extensions" / module.MODE_ID / "skills" / module.MODE_ID

    assert codex_target.is_dir() and not codex_target.is_symlink()
    assert claude_target.is_dir() and not claude_target.is_symlink()
    assert gemini_target.is_dir() and not gemini_target.is_symlink()


def test_symlink_mode_links_skill_directories(tmp_path):
    module = load_module()

    module.install_mode(home=tmp_path, copy_mode=False)

    codex_target = tmp_path / ".codex" / "skills" / module.MODE_ID
    claude_target = tmp_path / ".claude" / "skills" / module.MODE_ID
    gemini_target = tmp_path / ".gemini" / "extensions" / module.MODE_ID / "skills" / module.MODE_ID

    assert codex_target.is_symlink()
    assert claude_target.is_symlink()
    assert gemini_target.is_symlink()


def test_antigravity_target_uses_gemini_extension_install(tmp_path):
    module = load_module()

    module.install_mode(home=tmp_path, copy_mode=True, targets=("antigravity",))

    gemini_manifest = (
        tmp_path / ".gemini" / "extensions" / module.MODE_ID / "gemini-extension.json"
    )
    assert gemini_manifest.exists()
