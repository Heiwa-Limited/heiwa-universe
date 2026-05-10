from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


def test_root_keeps_only_discovery_and_compatibility_context_files():
    expected_root_files = {
        "AGENTS.md",
        "CLAUDE.md",
        "GEMINI.md",
        "README.md",
        "HEIWA.md",
        "SOUL.md",
        "IDENTITY.md",
        "justfile",
    }

    for name in expected_root_files:
        assert (ROOT / name).exists(), f"missing required root file {name}"

    retired_root_files = {
        "HEARTBEAT.md",
        "MEMORY.md",
        "PLAN.md",
        "TOOLS.md",
        "USER.md",
    }
    for name in retired_root_files:
        assert not (ROOT / name).exists(), f"{name} should move under ops/context"


def test_ops_context_contains_canonical_operator_files():
    ops_context = ROOT / "ops" / "context"
    expected = {
        "HEARTBEAT.md",
        "HEIWA.md",
        "IDENTITY.md",
        "MEMORY.md",
        "PLAN.md",
        "SOUL.md",
        "TOOLS.md",
        "USER.md",
    }

    for name in expected:
        assert (ops_context / name).exists(), f"missing canonical operator file {name}"


def test_ops_rooms_replaces_root_rooms_directory():
    expected_rooms = {
        "control-plane.md",
        "execution.md",
        "orchestration.md",
        "infra.md",
        "sdk.md",
    }

    assert not (ROOT / "rooms").exists(), "rooms should move under ops/"
    for name in expected_rooms:
        assert (ROOT / "ops" / "rooms" / name).exists(), f"missing ops room file {name}"


def test_ops_docs_and_deps_replaces_root_vendor_archive():
    assert not (ROOT / "docs_and_deps").exists(), "docs_and_deps should move under ops/"


def test_justfile_defines_product_graph_without_ops_or_skills():
    justfile = (ROOT / "justfile").read_text(encoding="utf-8")

    for recipe in ("test-hub:", "test-trading:", "check-web:", "check-docs:", "test-product:"):
        assert recipe in justfile

    assert "packages/heiwa_skills" not in justfile
    assert "docs/superpowers" not in justfile
    assert "ops/" not in justfile
