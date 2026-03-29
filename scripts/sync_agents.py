#!/usr/bin/env python3
"""Cross-runtime agent sync tool for Heiwa canonical agents.

Reads canonical agent definitions from ops/agents/ and generates
runtime-specific wrappers for Gemini, Claude, and Codex.

Usage:
    uv run scripts/sync_agents.py              # Generate all wrappers
    uv run scripts/sync_agents.py --check      # Verify wrappers are current
    uv run scripts/sync_agents.py --install-codex  # Symlink Codex wrappers
"""
from __future__ import annotations

import json

# -- Constants --

GEMINI_TOOLS_MAP: dict[str, list[str]] = {
    "full_access": ["*"],
    "read_only": [
        "read_file",
        "grep_search",
        "glob",
        "list_directory",
        "google_web_search",
    ],
}

CLAUDE_DISALLOWED_READ_ONLY: list[str] = ["Write", "Edit", "MultiEdit", "Bash"]

CODEX_READ_ONLY_POLICY = (
    "## Read-Only Policy\n"
    "\n"
    "This specialist operates in read-only mode. "
    "Do not modify files, run destructive commands, or commit changes.\n"
)


# -- Banner --


def generate_banner(agent_id: str) -> str:
    """Generate the GENERATED FILE banner for a wrapper."""
    return (
        f"<!-- GENERATED FILE - DO NOT EDIT\n"
        f"manifest: ops/agents/{agent_id}/agent.yaml\n"
        f"prompt: ops/agents/{agent_id}/prompt.md\n"
        f"regen: uv run scripts/sync_agents.py\n"
        f"-->"
    )


# -- Gemini --


def generate_gemini_wrapper(manifest: dict, prompt_body: str) -> str:
    """Generate a Gemini agent wrapper from canonical manifest + prompt."""
    agent_id = manifest["id"]
    target = manifest["targets"]["gemini"]
    tools = json.dumps(GEMINI_TOOLS_MAP[manifest["tool_profile"]])

    lines = [
        "---",
        f"name: {agent_id}",
        f"description: {manifest['description']}",
        f"tools: {tools}",
        f"model: {target['model']}",
        f"max_turns: {target['max_turns']}",
        "---",
        "",
        generate_banner(agent_id),
        "",
        prompt_body,
        "",
    ]
    return "\n".join(lines)


# -- Claude --


def generate_claude_wrapper(manifest: dict, prompt_body: str) -> str:
    """Generate a Claude agent wrapper from canonical manifest + prompt."""
    agent_id = manifest["id"]
    target = manifest["targets"]["claude"]

    fm_lines = [
        "---",
        f"name: {agent_id}",
        f"description: {manifest['description']}",
    ]
    if "model" in target:
        fm_lines.append(f"model: {target['model']}")
    if "max_turns" in target:
        fm_lines.append(f"maxTurns: {target['max_turns']}")
    if manifest["tool_profile"] == "read_only":
        fm_lines.append(f"disallowedTools: {json.dumps(CLAUDE_DISALLOWED_READ_ONLY)}")
    fm_lines.append("---")

    lines = [*fm_lines, "", generate_banner(agent_id), "", prompt_body, ""]
    return "\n".join(lines)


# -- Codex --


def generate_codex_wrapper(manifest: dict, prompt_body: str) -> str:
    """Generate a Codex SKILL.md wrapper from canonical manifest + prompt."""
    agent_id = manifest["id"]

    parts = [
        "---",
        f"name: {agent_id}",
        f"description: {manifest['description']}",
        "---",
        "",
        generate_banner(agent_id),
        "",
    ]

    if manifest["tool_profile"] == "read_only":
        parts.append(CODEX_READ_ONLY_POLICY)

    parts.extend([prompt_body, ""])
    return "\n".join(parts)
