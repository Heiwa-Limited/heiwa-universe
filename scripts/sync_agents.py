#!/usr/bin/env python3
"""Cross-runtime agent sync tool for Heiwa canonical agents.

Reads canonical agent definitions from ops/agents/ and generates
runtime-specific wrappers for Gemini, Claude, and Codex.

Usage:
    uv run scripts/sync_agents.py                  # Generate all wrappers
    uv run scripts/sync_agents.py --check          # Verify wrappers are current
    uv run scripts/sync_agents.py --install-codex  # Symlink Codex wrappers into ~/.codex/skills
"""
from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path

import yaml

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


import tomllib

# -- Config parity constants --

REQUIRED_CODEX_MCP = {
    "MCP_DOCKER", "playwright", "railway", "figma", "notion", "codebase-retrieval",
}
REQUIRED_CODEX_PLUGINS = {"github", "cloudflare", "google-drive", "hugging-face"}
REQUIRED_CODEX_FEATURES = {"multi_agent", "prevent_idle_sleep"}


def check_wrapper_drift(agents: list[dict]) -> list[str]:
    """Check all generated wrappers for drift and orphans."""
    errors: list[str] = []
    managed_gemini: set[str] = set()
    managed_claude: set[str] = set()

    for agent in agents:
        m = agent["manifest"]
        p = agent["prompt_body"]

        for runtime, target in m["targets"].items():
            if not target.get("enabled", False):
                continue

            if runtime == "gemini":
                path = REPO_ROOT / target["output"]
                managed_gemini.add(path.name)
                expected = generate_gemini_wrapper(m, p)
                if not path.exists():
                    errors.append(f"MISSING: {path.relative_to(REPO_ROOT)}")
                elif path.read_text() != expected:
                    errors.append(f"DRIFT: {path.relative_to(REPO_ROOT)}")

            elif runtime == "claude":
                path = REPO_ROOT / target["output"]
                managed_claude.add(path.name)
                expected = generate_claude_wrapper(m, p)
                if not path.exists():
                    errors.append(f"MISSING: {path.relative_to(REPO_ROOT)}")
                elif path.read_text() != expected:
                    errors.append(f"DRIFT: {path.relative_to(REPO_ROOT)}")

            elif runtime == "codex":
                skill_path = REPO_ROOT / target["generated_dir"] / "SKILL.md"
                expected = generate_codex_wrapper(m, p)
                if not skill_path.exists():
                    errors.append(f"MISSING: {skill_path.relative_to(REPO_ROOT)}")
                elif skill_path.read_text() != expected:
                    errors.append(f"DRIFT: {skill_path.relative_to(REPO_ROOT)}")

    # Orphan detection — Gemini
    if GEMINI_AGENTS_DIR.exists():
        for f in GEMINI_AGENTS_DIR.glob("*.md"):
            if f.name not in managed_gemini:
                errors.append(f"ORPHAN: {f.relative_to(REPO_ROOT)}")

    # Orphan detection — Claude
    if CLAUDE_AGENTS_DIR.exists():
        for f in CLAUDE_AGENTS_DIR.glob("*.md"):
            if f.name not in managed_claude:
                errors.append(f"ORPHAN: {f.relative_to(REPO_ROOT)}")

    return errors


def check_codex_config() -> list[str]:
    """Verify .codex/config.toml declares all Heiwa-required surfaces."""
    errors: list[str] = []
    config_path = REPO_ROOT / ".codex" / "config.toml"

    if not config_path.exists():
        return ["MISSING: .codex/config.toml"]

    with open(config_path, "rb") as f:
        config = tomllib.load(f)

    mcp_keys = set(config.get("mcp_servers", {}).keys())
    for required in sorted(REQUIRED_CODEX_MCP):
        if required not in mcp_keys:
            errors.append(f"CODEX CONFIG: missing MCP server '{required}'")

    plugin_names: set[str] = set()
    for key, val in config.get("plugins", {}).items():
        if val.get("enabled", False):
            plugin_names.add(key.split("@")[0])
    for required in sorted(REQUIRED_CODEX_PLUGINS):
        if required not in plugin_names:
            errors.append(f"CODEX CONFIG: missing plugin '{required}'")

    features = config.get("features", {})
    for required in sorted(REQUIRED_CODEX_FEATURES):
        if not features.get(required, False):
            errors.append(f"CODEX CONFIG: missing feature '{required}'")
    if features.get("guardian_approval"):
        errors.append("CODEX CONFIG: guardian_approval must be false for provider-owned subagent orchestration")

    if config.get("approval_policy") != "on-request":
        errors.append("CODEX CONFIG: approval_policy must be 'on-request'")
    if config.get("sandbox_mode") != "workspace-write":
        errors.append("CODEX CONFIG: sandbox_mode must be 'workspace-write'")

    return errors


def check_claude_config() -> list[str]:
    """Verify .claude/settings.json has required Heiwa keys."""
    errors: list[str] = []
    config_path = REPO_ROOT / ".claude" / "settings.json"

    if not config_path.exists():
        return ["MISSING: .claude/settings.json"]

    with open(config_path) as f:
        config = json.load(f)

    if not config.get("enableAllProjectMcpServers"):
        errors.append("CLAUDE CONFIG: enableAllProjectMcpServers not true")

    if not config.get("enabledPlugins"):
        errors.append("CLAUDE CONFIG: no enabledPlugins defined")

    return errors


def check_gemini_config() -> list[str]:
    """Verify .gemini/settings.json has required Heiwa keys."""
    errors: list[str] = []
    config_path = REPO_ROOT / ".gemini" / "settings.json"

    if not config_path.exists():
        return ["MISSING: .gemini/settings.json"]

    with open(config_path) as f:
        config = json.load(f)

    general = config.get("general", {})
    if "defaultApprovalMode" not in general:
        errors.append("GEMINI CONFIG: missing general.defaultApprovalMode")
    elif general.get("defaultApprovalMode") != "auto_edit":
        errors.append("GEMINI CONFIG: general.defaultApprovalMode must be 'auto_edit'")

    security = config.get("security", {})
    if not security.get("environmentVariableRedaction", {}).get("enabled"):
        errors.append("GEMINI CONFIG: environmentVariableRedaction not enabled")
    if not security.get("enablePermanentToolApproval"):
        errors.append("GEMINI CONFIG: enablePermanentToolApproval not enabled")

    filtering = config.get("context", {}).get("fileFiltering", {})
    if not filtering.get("respectGitIgnore"):
        errors.append("GEMINI CONFIG: respectGitIgnore not enabled")

    experimental = config.get("experimental", {})
    if not experimental.get("enableAgents"):
        errors.append("GEMINI CONFIG: experimental.enableAgents not enabled")

    return errors


def cmd_check(agents: list[dict]) -> bool:
    """Run all verification checks. Returns True if clean."""
    all_errors: list[str] = []

    all_errors.extend(check_wrapper_drift(agents))
    all_errors.extend(check_codex_config())
    all_errors.extend(check_claude_config())
    all_errors.extend(check_gemini_config())

    if all_errors:
        print(f"CHECK FAILED — {len(all_errors)} error(s):", file=sys.stderr)
        for e in all_errors:
            print(f"  {e}", file=sys.stderr)
        return False

    print("All checks passed.")
    return True


import shutil

DEFAULT_SKILLS_DIR = Path.home() / ".codex" / "skills"


def cmd_install_codex(
    agents: list[dict],
    skills_dir: Path | None = None,
    copy_mode: bool = False,
) -> None:
    """Install Codex wrappers into the native discovery path."""
    if skills_dir is None:
        skills_dir = DEFAULT_SKILLS_DIR
    skills_dir.mkdir(parents=True, exist_ok=True)

    for agent in agents:
        m = agent["manifest"]
        codex = m["targets"].get("codex", {})
        if not codex.get("enabled"):
            continue

        install_name = codex["install_name"]
        generated_dir = REPO_ROOT / codex["generated_dir"]
        install_target = skills_dir / install_name

        if copy_mode:
            if install_target.exists():
                shutil.rmtree(install_target)
            shutil.copytree(generated_dir, install_target)
        else:
            if install_target.is_symlink() or install_target.exists():
                if install_target.is_symlink():
                    install_target.unlink()
                else:
                    shutil.rmtree(install_target)
            install_target.symlink_to(generated_dir.resolve())

        print(f"  Installed {install_name} → {install_target}")


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
        ok = cmd_check(agents)
        return 0 if ok else 1

    if args.install_codex:
        cmd_install_codex(agents, copy_mode=args.copy)
        print("Done.")
        return 0

    cmd_generate(agents)
    print("Done.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
