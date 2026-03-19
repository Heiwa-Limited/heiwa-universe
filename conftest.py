"""
Root conftest.py — PYTHONPATH setup for pytest.

This replaces the manual sys.path manipulation that each individual test file
does at the top (the ROOT = Path(__file__).resolve().parents[3] + sys.path.insert
pattern). With this conftest in place, pytest automatically discovers it and
configures paths before any test imports run.

Mirrors the export PYTHONPATH line from CLAUDE.md / CI env:
  packages/heiwa_cli:packages/heiwa_cognition:packages/heiwa_sdk:
  packages/heiwa_protocol:packages/heiwa_identity:packages/heiwa_ui:
  packages/heiwa_knowledge:apps
"""
from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent

_path_entries = [
    ROOT / "packages" / "heiwa_cli",
    ROOT / "packages" / "heiwa_cognition",
    ROOT / "packages" / "heiwa_sdk",
    ROOT / "packages" / "heiwa_protocol",
    ROOT / "packages" / "heiwa_identity",
    ROOT / "packages" / "heiwa_ui",
    ROOT / "packages" / "heiwa_knowledge",
    ROOT / "apps",
]

for entry in reversed(_path_entries):
    entry_str = str(entry)
    if entry_str not in sys.path:
        sys.path.insert(0, entry_str)
