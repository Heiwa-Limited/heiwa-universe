"""Tests for the cross-runtime agent sync tool."""
from __future__ import annotations

from sync_agents import (
    generate_banner,
    generate_claude_wrapper,
    generate_codex_wrapper,
    generate_gemini_wrapper,
)

# -- Fixtures --

FULL_ACCESS_MANIFEST = {
    "id": "heiwa-architect",
    "name": "Heiwa Architect",
    "description": "Specialized architect for Heiwa state, mesh connectivity, and protocol changes.",
    "tool_profile": "full_access",
    "targets": {
        "gemini": {"enabled": True, "model": "auto-gemini-3", "max_turns": 15},
        "claude": {"enabled": True, "model": "sonnet", "max_turns": 15},
        "codex": {"enabled": True},
    },
}

READ_ONLY_MANIFEST = {
    "id": "heiwa-researcher",
    "name": "Heiwa Researcher",
    "description": "Read-only codebase investigator for Heiwa.",
    "tool_profile": "read_only",
    "targets": {
        "gemini": {"enabled": True, "model": "auto-gemini-3", "max_turns": 15},
        "claude": {"enabled": True, "model": "sonnet", "max_turns": 15},
        "codex": {"enabled": True},
    },
}

PROMPT_BODY = "# Test Agent\n\nYou are a test agent."


# -- Banner tests --


def test_banner_contains_manifest_path():
    banner = generate_banner("heiwa-architect")
    assert "ops/agents/heiwa-architect/agent.yaml" in banner


def test_banner_contains_prompt_path():
    banner = generate_banner("heiwa-architect")
    assert "ops/agents/heiwa-architect/prompt.md" in banner


def test_banner_contains_regen_command():
    banner = generate_banner("heiwa-architect")
    assert "uv run scripts/sync_agents.py" in banner


def test_banner_starts_with_generated_warning():
    banner = generate_banner("heiwa-architect")
    assert banner.startswith("<!-- GENERATED FILE - DO NOT EDIT")


# -- Gemini wrapper tests --


def test_gemini_full_access_has_wildcard_tools():
    result = generate_gemini_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    assert 'tools: ["*"]' in result


def test_gemini_read_only_has_restricted_tools():
    result = generate_gemini_wrapper(READ_ONLY_MANIFEST, PROMPT_BODY)
    assert "read_file" in result
    assert "grep_search" in result
    assert '"*"' not in result


def test_gemini_wrapper_has_model_and_turns():
    result = generate_gemini_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    assert "model: auto-gemini-3" in result
    assert "max_turns: 15" in result


def test_gemini_wrapper_contains_prompt_body():
    result = generate_gemini_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    assert PROMPT_BODY in result


def test_gemini_wrapper_contains_banner():
    result = generate_gemini_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    assert "GENERATED FILE - DO NOT EDIT" in result


# -- Claude wrapper tests --


def test_claude_full_access_omits_disallowed_tools():
    result = generate_claude_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    assert "disallowedTools" not in result


def test_claude_read_only_has_disallowed_tools():
    result = generate_claude_wrapper(READ_ONLY_MANIFEST, PROMPT_BODY)
    assert "disallowedTools" in result
    assert '"Write"' in result
    assert '"Edit"' in result
    assert '"MultiEdit"' in result
    assert '"Bash"' in result


def test_claude_wrapper_uses_camel_case_max_turns():
    result = generate_claude_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    assert "maxTurns: 15" in result
    assert "max_turns" not in result


def test_claude_wrapper_contains_prompt_body():
    result = generate_claude_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    assert PROMPT_BODY in result


def test_claude_wrapper_contains_banner():
    result = generate_claude_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    assert "GENERATED FILE - DO NOT EDIT" in result


# -- Codex wrapper tests --


def test_codex_full_access_has_no_policy_section():
    result = generate_codex_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    assert "Read-Only Policy" not in result


def test_codex_read_only_has_policy_section():
    result = generate_codex_wrapper(READ_ONLY_MANIFEST, PROMPT_BODY)
    assert "## Read-Only Policy" in result
    assert "read-only mode" in result


def test_codex_wrapper_has_name_and_description_only():
    result = generate_codex_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    # Codex frontmatter should not include model or max_turns
    assert "model:" not in result.split("---")[1]
    assert "max_turns" not in result.split("---")[1]


def test_codex_wrapper_contains_prompt_body():
    result = generate_codex_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    assert PROMPT_BODY in result


def test_codex_wrapper_contains_banner():
    result = generate_codex_wrapper(FULL_ACCESS_MANIFEST, PROMPT_BODY)
    assert "GENERATED FILE - DO NOT EDIT" in result


import os
from pathlib import Path

import yaml

from sync_agents import load_registry, REPO_ROOT


def test_load_registry_returns_five_agents():
    agents = load_registry()
    assert len(agents) == 5


def test_load_registry_agents_have_manifest_and_prompt():
    agents = load_registry()
    for agent in agents:
        assert "manifest" in agent
        assert "prompt_body" in agent
        assert agent["manifest"]["id"]
        assert len(agent["prompt_body"]) > 0


def test_load_registry_researcher_is_read_only():
    agents = load_registry()
    researcher = [a for a in agents if a["manifest"]["id"] == "heiwa-researcher"][0]
    assert researcher["manifest"]["tool_profile"] == "read_only"


import tomllib

from sync_agents import (
    check_codex_config,
    check_claude_config,
    check_gemini_config,
    check_wrapper_drift,
)


def test_check_wrapper_drift_clean_passes():
    """After a fresh sync, drift check should find zero errors."""
    agents = load_registry()
    errors = check_wrapper_drift(agents)
    assert errors == []


def test_check_codex_config_passes_after_parity_fix():
    """After config parity fix, Codex config check should pass clean."""
    errors = check_codex_config()
    assert errors == [], f"Unexpected Codex config errors: {errors}"


def test_check_claude_config_passes():
    """Claude config should already have enableAllProjectMcpServers."""
    errors = check_claude_config()
    mcp_errors = [e for e in errors if "enableAllProjectMcpServers" in e]
    assert mcp_errors == []


def test_check_gemini_config_passes():
    """Gemini config should already have required keys."""
    errors = check_gemini_config()
    assert errors == []


from sync_agents import cmd_install_codex


def test_install_codex_creates_symlinks(tmp_path):
    """Verify install creates symlinks from target dir to generated source."""
    agents = load_registry()
    cmd_install_codex(agents, skills_dir=tmp_path)

    for agent in agents:
        m = agent["manifest"]
        codex = m["targets"].get("codex", {})
        if not codex.get("enabled"):
            continue
        install_name = codex["install_name"]
        link = tmp_path / install_name
        assert link.is_symlink(), f"{install_name} should be a symlink"
        assert (link / "SKILL.md").exists(), f"{install_name}/SKILL.md should exist"


def test_install_codex_copy_mode(tmp_path):
    """Verify --copy creates real directories instead of symlinks."""
    agents = load_registry()
    cmd_install_codex(agents, skills_dir=tmp_path, copy_mode=True)

    for agent in agents:
        m = agent["manifest"]
        codex = m["targets"].get("codex", {})
        if not codex.get("enabled"):
            continue
        install_name = codex["install_name"]
        target = tmp_path / install_name
        assert target.is_dir() and not target.is_symlink()
        assert (target / "SKILL.md").exists()
