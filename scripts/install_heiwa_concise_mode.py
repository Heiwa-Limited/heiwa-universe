#!/usr/bin/env python3
"""Install the Heiwa provider-agnostic concise mode into local agent surfaces."""
from __future__ import annotations

import argparse
import json
import shutil
from pathlib import Path

MODE_ID = "heiwa-concise-mode"
MODE_VERSION = "0.1.0"
UPSTREAM_REPO = "https://github.com/JuliusBrussee/caveman"
UPSTREAM_RELEASE = "v1.3.5"

REPO_ROOT = Path(__file__).resolve().parent.parent
SOURCE_DIR = REPO_ROOT / "packages" / "heiwa_skills" / MODE_ID
SKILL_FILE = SOURCE_DIR / "SKILL.md"
MODE_FILE = SOURCE_DIR / "MODE.md"
README_FILE = SOURCE_DIR / "README.md"


def render_gemini_extension_manifest() -> dict[str, str]:
    return {
        "name": MODE_ID,
        "description": "Provider-agnostic concise response mode for Heiwa operator work",
        "version": MODE_VERSION,
        "contextFileName": "GEMINI.md",
    }


def render_heiwa_manifest() -> dict[str, object]:
    return {
        "id": MODE_ID,
        "name": "Heiwa Concise Mode",
        "version": MODE_VERSION,
        "provider_agnostic": True,
        "model_agnostic": True,
        "targets": ["codex", "claude", "gemini", "antigravity", "heiwa", "ollama"],
        "upstream": {
            "repo": UPSTREAM_REPO,
            "release": UPSTREAM_RELEASE,
        },
    }


def remove_target(path: Path) -> None:
    if not path.exists() and not path.is_symlink():
        return
    if path.is_symlink() or path.is_file():
        path.unlink()
        return
    shutil.rmtree(path)


def install_tree(source: Path, target: Path, copy_mode: bool) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    remove_target(target)
    if copy_mode:
        shutil.copytree(source, target)
    else:
        target.symlink_to(source.resolve(), target_is_directory=True)


def install_file(source: Path, target: Path, copy_mode: bool) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    remove_target(target)
    if copy_mode:
        shutil.copy2(source, target)
    else:
        target.symlink_to(source.resolve())


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)


def install_codex(home: Path, copy_mode: bool) -> None:
    install_tree(SOURCE_DIR, home / ".codex" / "skills" / MODE_ID, copy_mode)


def install_claude(home: Path, copy_mode: bool) -> None:
    install_tree(SOURCE_DIR, home / ".claude" / "skills" / MODE_ID, copy_mode)


def install_gemini(home: Path, copy_mode: bool) -> None:
    extension_root = home / ".gemini" / "extensions" / MODE_ID
    remove_target(extension_root)
    extension_root.mkdir(parents=True, exist_ok=True)

    write_text(
        extension_root / "gemini-extension.json",
        json.dumps(render_gemini_extension_manifest(), indent=2) + "\n",
    )
    write_text(extension_root / "GEMINI.md", f"@./skills/{MODE_ID}/SKILL.md\n")
    install_tree(SOURCE_DIR, extension_root / "skills" / MODE_ID, copy_mode)


def install_heiwa(home: Path, copy_mode: bool) -> None:
    mode_root = home / ".heiwa" / "modes" / MODE_ID
    remove_target(mode_root)
    mode_root.mkdir(parents=True, exist_ok=True)

    write_text(mode_root / "manifest.json", json.dumps(render_heiwa_manifest(), indent=2) + "\n")
    install_file(MODE_FILE, mode_root / "MODE.md", copy_mode)
    install_file(README_FILE, mode_root / "README.md", copy_mode)


def install_mode(
    home: Path | None = None,
    copy_mode: bool = False,
    targets: tuple[str, ...] = ("codex", "claude", "gemini", "heiwa"),
) -> None:
    if home is None:
        home = Path.home()

    installers = {
        "codex": install_codex,
        "claude": install_claude,
        "gemini": install_gemini,
        "antigravity": install_gemini,
        "heiwa": install_heiwa,
    }

    for target in targets:
        installers[target](home, copy_mode)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Install Heiwa concise mode")
    parser.add_argument(
        "--home",
        type=Path,
        default=Path.home(),
        help="Target home directory override",
    )
    parser.add_argument(
        "--copy",
        action="store_true",
        help="Copy files instead of creating symlinks where possible",
    )
    parser.add_argument(
        "--targets",
        default="codex,claude,gemini,heiwa",
        help="Comma-separated install targets",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    targets = tuple(part.strip() for part in args.targets.split(",") if part.strip())
    install_mode(home=args.home, copy_mode=args.copy, targets=targets)
    for target in targets:
        print(f"installed {MODE_ID} -> {target}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
