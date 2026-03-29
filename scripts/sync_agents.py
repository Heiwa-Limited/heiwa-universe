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


import argparse
import sys
from pathlib import Path

import yaml

REPO_ROOT = Path(__file__).resolve().parent.parent
AGENTS_DIR = REPO_ROOT / "ops" / "agents"
REGISTRY_FILE = AGENTS_DIR / "registry.yaml"
GEMINI_AGENTS_DIR = REPO_ROOT / ".gemini" / "agents"
CLAUDE_AGENTS_DIR = REPO_ROOT / ".claude" / "agents"


def load_registry() -> list[dict]:
    """Load canonical agent registry and return manifests with prompt bodies."""
    with open(REGISTRY_FILE) as f:
        registry = yaml.safe_load(f)

    agents = []
    for entry in registry["agents"]:
        agent_id = entry["id"]
        agent_dir = AGENTS_DIR / agent_id

        with open(agent_dir / "agent.yaml") as f:
            manifest = yaml.safe_load(f)

        prompt_file = agent_dir / manifest.get("prompt_file", "prompt.md")
        prompt_body = prompt_file.read_text().rstrip("\n")

        agents.append({"manifest": manifest, "prompt_body": prompt_body})

    return agents


def cmd_generate(agents: list[dict]) -> None:
    """Generate all runtime wrappers from canonical sources."""
    for agent in agents:
        m = agent["manifest"]
        p = agent["prompt_body"]

        for runtime, target in m["targets"].items():
            if not target.get("enabled", False):
                continue

            if runtime == "gemini":
                output = REPO_ROOT / target["output"]
                output.parent.mkdir(parents=True, exist_ok=True)
                output.write_text(generate_gemini_wrapper(m, p))
                print(f"  Generated {output.relative_to(REPO_ROOT)}")

            elif runtime == "claude":
                output = REPO_ROOT / target["output"]
                output.parent.mkdir(parents=True, exist_ok=True)
                output.write_text(generate_claude_wrapper(m, p))
                print(f"  Generated {output.relative_to(REPO_ROOT)}")

            elif runtime == "codex":
                output_dir = REPO_ROOT / target["generated_dir"]
                output_dir.mkdir(parents=True, exist_ok=True)
                (output_dir / "SKILL.md").write_text(generate_codex_wrapper(m, p))
                print(f"  Generated {(output_dir / 'SKILL.md').relative_to(REPO_ROOT)}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Heiwa canonical agent sync tool")
    parser.add_argument("--check", action="store_true", help="Verify wrappers are current")
    parser.add_argument("--install-codex", action="store_true", help="Install Codex wrappers")
    parser.add_argument("--copy", action="store_true", help="Copy instead of symlink for Codex install")
    args = parser.parse_args()

    agents = load_registry()

    if args.check:
        print("Check mode not yet implemented.")
        return 1

    if args.install_codex:
        print("Install mode not yet implemented.")
        return 1

    cmd_generate(agents)
    print("Done.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
