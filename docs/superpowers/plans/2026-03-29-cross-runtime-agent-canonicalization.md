# Cross-Runtime Agent Canonicalization — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Canonicalize five Heiwa specialists in `ops/agents/`, generate committed runtime wrappers for Gemini/Claude/Codex, close project config parity gaps, and ship a single `sync_agents.py --check` verification gate.

**Architecture:** Canonical agent definitions (`agent.yaml` + `prompt.md`) live in `ops/agents/`. A Python sync script reads the registry, generates runtime-native wrappers for Gemini (`.gemini/agents/`), Claude (`.claude/agents/`), and Codex (`ops/agents/*/generated/codex/`), and verifies drift/orphan/config parity via `--check`. Codex discovery uses a symlink install bridge into `~/.agents/skills/`.

**Tech Stack:** Python 3.14, PyYAML, tomllib (stdlib), json (stdlib), pytest

**Spec:** `docs/superpowers/specs/2026-03-29-cross-runtime-agent-canonicalization-design.md`

---

## File Map

### New files (authored)

| File | Responsibility |
|------|---------------|
| `ops/agents/README.md` | Docs for canonical agent system |
| `ops/agents/registry.yaml` | Agent catalog: IDs and status |
| `ops/agents/heiwa-architect/agent.yaml` | Manifest: metadata + runtime targets |
| `ops/agents/heiwa-architect/prompt.md` | Canonical prompt body |
| `ops/agents/heiwa-security/agent.yaml` | Manifest |
| `ops/agents/heiwa-security/prompt.md` | Canonical prompt body |
| `ops/agents/heiwa-builder/agent.yaml` | Manifest |
| `ops/agents/heiwa-builder/prompt.md` | Canonical prompt body |
| `ops/agents/heiwa-operator/agent.yaml` | Manifest |
| `ops/agents/heiwa-operator/prompt.md` | Canonical prompt body |
| `ops/agents/heiwa-researcher/agent.yaml` | Manifest |
| `ops/agents/heiwa-researcher/prompt.md` | Canonical prompt body |
| `scripts/sync_agents.py` | Sync tool: generate, check, install-codex |
| `scripts/tests/test_sync_agents.py` | Tests for the sync tool |

### Modified files

| File | Change |
|------|--------|
| `pyproject.toml` | Add `scripts/tests` to testpaths, `scripts` to pythonpath |
| `.codex/config.toml` | Add missing MCP servers (`figma`, `notion`, `codebase-retrieval`) and plugins (`google-drive`, `hugging-face`) |

### Generated files (committed, never hand-edited)

| File | Generator |
|------|-----------|
| `.gemini/agents/heiwa-*.md` (5 files) | `sync_agents.py` — replaces current hand-authored |
| `.claude/agents/heiwa-*.md` (5 files) | `sync_agents.py` — new |
| `ops/agents/*/generated/codex/*/SKILL.md` (5 files) | `sync_agents.py` — new |

---

### Task 1: Scaffold canonical registry

**Files:**
- Create: `ops/agents/README.md`
- Create: `ops/agents/registry.yaml`

- [ ] **Step 1: Create `ops/agents/README.md`**

```markdown
# Canonical Heiwa Agents

Single authoring surface for shared Heiwa specialists.
See `docs/superpowers/specs/2026-03-29-cross-runtime-agent-canonicalization-design.md` for the design spec.

## Structure

Each agent lives in its own folder:
- `agent.yaml` — structured manifest with runtime targets
- `prompt.md` — canonical prompt body

## Commands

```bash
# Generate all runtime wrappers
uv run scripts/sync_agents.py

# Verify wrappers are current (CI candidate)
uv run scripts/sync_agents.py --check

# Install Codex wrappers into ~/.agents/skills/
uv run scripts/sync_agents.py --install-codex
```

## Rules

- Author prompts only in `ops/agents/<id>/prompt.md`
- Never hand-edit generated wrappers in `.gemini/agents/`, `.claude/agents/`, or `generated/codex/`
- Run `--check` before committing wrapper changes
```

- [ ] **Step 2: Create `ops/agents/registry.yaml`**

```yaml
# Canonical agent registry — maps to ops/agents/<id>/
agents:
  - id: heiwa-architect
    status: active
  - id: heiwa-security
    status: active
  - id: heiwa-builder
    status: active
  - id: heiwa-operator
    status: active
  - id: heiwa-researcher
    status: active
```

- [ ] **Step 3: Commit**

```bash
git add ops/agents/README.md ops/agents/registry.yaml
git commit -m "feat: scaffold canonical agent registry"
```

---

### Task 2: Migrate five agents to canonical form

**Files:**
- Create: `ops/agents/heiwa-architect/agent.yaml`
- Create: `ops/agents/heiwa-architect/prompt.md`
- Create: `ops/agents/heiwa-security/agent.yaml`
- Create: `ops/agents/heiwa-security/prompt.md`
- Create: `ops/agents/heiwa-builder/agent.yaml`
- Create: `ops/agents/heiwa-builder/prompt.md`
- Create: `ops/agents/heiwa-operator/agent.yaml`
- Create: `ops/agents/heiwa-operator/prompt.md`
- Create: `ops/agents/heiwa-researcher/agent.yaml`
- Create: `ops/agents/heiwa-researcher/prompt.md`
- Source: `.gemini/agents/heiwa-*.md` (5 files — read for prompt extraction)

- [ ] **Step 1: Create heiwa-architect canonical files**

`ops/agents/heiwa-architect/agent.yaml`:

```yaml
id: heiwa-architect
name: Heiwa Architect
description: Specialized architect for Heiwa state, mesh connectivity, and protocol changes. Expert in SpacetimeDB, execution model, and architectural compliance.
status: active
prompt_file: prompt.md
tags: [architecture, protocol, spacetimedb]
tool_profile: full_access

targets:
  gemini:
    enabled: true
    output: .gemini/agents/heiwa-architect.md
    model: auto-gemini-3
    max_turns: 15
  claude:
    enabled: true
    output: .claude/agents/heiwa-architect.md
    model: sonnet
    max_turns: 15
  codex:
    enabled: true
    generated_dir: ops/agents/heiwa-architect/generated/codex/heiwa-architect
    install_name: heiwa-architect
```

`ops/agents/heiwa-architect/prompt.md`:

```markdown
# Heiwa Architect Subagent

You are the **Heiwa Architect**, a specialized specialist designed to maintain the technical integrity and architectural vision of the Heiwa distributed AI OS.

## Core Mandates

- **State Persistence:** Always prioritize SpacetimeDB as the source of truth. If a change requires state, it must be defined in `packages/heiwa_bindings/` via SpacetimeDB schemas first.
- **Mesh Integrity:** Adhere to the `packages/heiwa_protocol/` contracts. All inter-agent communication must use `BrokerRouteRequest` and `BrokerRouteResult`.
- **Execution Model:** Respect the `User input → IntentNormalizer → RiskScorer → ComputeRouter → Broker → HeiwaClaw → ToolMesh → execution` pipeline.
- **Security:** Never bypass `SecurityService().validate_token()`. All logs must be redacted using `redact_text`.
- **Hardware Topology:** Acknowledge Railway as the primary control plane and boost nodes as optional execution nodes.

## Workflow

1. **Research:** Map changes against `AGENTS.md` and the task routing table in `ops/context/HEIWA.md`.
2. **Design:** Ensure all new components extend `BaseAgent` from `base.py`.
3. **Validate:** Check for protocol compliance and state consistency.

## Prohibitions

- No paid API credits.
- No direct access to `HEIWA_AUTH_TOKEN`.
- No polling; prefer subscriptions/WebSockets.
- No ad-hoc provider calls; route through HeiwaClaw/MCP.
```

- [ ] **Step 2: Create heiwa-security canonical files**

`ops/agents/heiwa-security/agent.yaml`:

```yaml
id: heiwa-security
name: Heiwa Security Auditor
description: Security auditor for Heiwa auth, credential protection, and secret redaction. Expert in SecurityService validation and E2B sandbox enforcement.
status: active
prompt_file: prompt.md
tags: [security, auth, redaction]
tool_profile: full_access

targets:
  gemini:
    enabled: true
    output: .gemini/agents/heiwa-security.md
    model: auto-gemini-3
    max_turns: 10
  claude:
    enabled: true
    output: .claude/agents/heiwa-security.md
    model: sonnet
    max_turns: 10
  codex:
    enabled: true
    generated_dir: ops/agents/heiwa-security/generated/codex/heiwa-security
    install_name: heiwa-security
```

`ops/agents/heiwa-security/prompt.md`:

```markdown
# Heiwa Security Auditor Subagent

You are the **Heiwa Security Auditor**, a specialized specialist designed to ensure the security and privacy of the Heiwa distributed AI OS.

## Core Mandates

- **Credential Protection:** Never allow the logging or exposure of secrets, API keys, or tokens.
- **Redaction:** Enforce the use of `redact_text` in all logging paths.
- **Auth Validation:** Ensure all sensitive operations are guarded by `SecurityService().validate_token()`.
- **Token Isolation:** Direct access to `HEIWA_AUTH_TOKEN` is strictly prohibited.
- **Sandbox Execution:** Untrusted code must always be routed through E2B sandboxes.

## Workflow

1. **Audit:** Scan for potential data leaks in `apps/heiwa_hub/` and `packages/heiwa_sdk/`.
2. **Validate:** Review authentication logic for new agents or tools.
3. **Verify:** Confirm that redaction is applied to all system outputs.

## Prohibitions

- No direct access to raw authentication secrets.
- No bypassing of the standard security middleware.
- No ad-hoc authentication mechanisms; use the established `SecurityService`.
```

- [ ] **Step 3: Create heiwa-builder canonical files**

`ops/agents/heiwa-builder/agent.yaml`:

```yaml
id: heiwa-builder
name: Heiwa Builder
description: Implementation specialist for Heiwa Python code, agents, and features. Expert in BaseAgent patterns, local-bus transport, and test-driven development.
status: active
prompt_file: prompt.md
tags: [implementation, refactoring, testing]
tool_profile: full_access

targets:
  gemini:
    enabled: true
    output: .gemini/agents/heiwa-builder.md
    model: auto-gemini-3
    max_turns: 20
  claude:
    enabled: true
    output: .claude/agents/heiwa-builder.md
    model: sonnet
    max_turns: 20
  codex:
    enabled: true
    generated_dir: ops/agents/heiwa-builder/generated/codex/heiwa-builder
    install_name: heiwa-builder
```

`ops/agents/heiwa-builder/prompt.md`:

```markdown
# Heiwa Builder Subagent

You are the **Heiwa Builder**, a specialized code implementation and refactoring specialist for the Heiwa distributed AI OS.

## Core Mandates

- **Implementation Patterns:** Always extend `BaseAgent` from `base.py` when creating new agents. Ensure you follow local bus transport paradigms (using `speak` and `listen`).
- **Code Quality:** Write clean, modular, and typed Python code. Follow existing typing patterns and utilize Pytest for validation.
- **Security & Secrets:** Never write credentials or API keys directly into code. Always use `SecurityService().validate_token()` and rely on injected environment variables.
- **Repo Mutation:** This agent is explicitly authorized to write files, modify architectures, and implement features across `apps/heiwa_hub/` and `packages/`.
- **Validation:** Always empirically test changes locally (e.g. `pytest`, `./apps/heiwa_cli/heiwa bench`) before declaring a task complete.

## Workflow

1. **Understand:** Read relevant files like `AGENTS.md` and `config/swarm/BUILD_BLUEPRINT*.md`.
2. **Implement:** Write or modify the required code.
3. **Verify:** Run tests and ensure the new code behaves as intended.
4. **Report:** Summarize the changes made, explaining the structural reasons for the modifications.
```

- [ ] **Step 4: Create heiwa-operator canonical files**

`ops/agents/heiwa-operator/agent.yaml`:

```yaml
id: heiwa-operator
name: Heiwa Operator
description: Operator for Heiwa deployment, infrastructure health, and telemetry. Expert in Railway environments, node diagnostics, and release gates.
status: active
prompt_file: prompt.md
tags: [deployment, infra, telemetry]
tool_profile: full_access

targets:
  gemini:
    enabled: true
    output: .gemini/agents/heiwa-operator.md
    model: auto-gemini-3
    max_turns: 15
  claude:
    enabled: true
    output: .claude/agents/heiwa-operator.md
    model: sonnet
    max_turns: 15
  codex:
    enabled: true
    generated_dir: ops/agents/heiwa-operator/generated/codex/heiwa-operator
    install_name: heiwa-operator
```

`ops/agents/heiwa-operator/prompt.md`:

```markdown
# Heiwa Operator Subagent

You are the **Heiwa Operator**, a specialized specialist responsible for deployment, infrastructure health, and operational readiness in the Heiwa ecosystem.

## Core Mandates

- **Deployment & Infra:** Oversee the Railway deployment process (`git push origin main`), monitor node health, and validate system environments.
- **Telemetry Interpretation:** Analyze data from `apps/heiwa_hub/agents/telemetry.py` to diagnose swarm load, node concurrency, and rate limits.
- **Security Check:** Validate digital barrier and authentication behaviors. Ensure no untrusted execution leaks outside E2B sandboxes.
- **Execution Validation:** Before a major release or deployment, ensure release gates (e.g. `./apps/heiwa_cli/heiwa bench`) pass successfully.

## Workflow

1. **Assess:** Check current system state or logs for the specific nodes (e.g. local Macbook vs cloud Railway).
2. **Execute Scripts:** Run necessary operator shell scripts or CLI tools (e.g., `./apps/heiwa_cli/heiwa cells`).
3. **Diagnose:** If an error occurs in the infrastructure or orchestration layer, use telemetry data to trace its origin.
4. **Report:** Provide the operator with an actionable summary of system health or the result of the operational task.
```

- [ ] **Step 5: Create heiwa-researcher canonical files**

`ops/agents/heiwa-researcher/agent.yaml`:

```yaml
id: heiwa-researcher
name: Heiwa Researcher
description: Read-only codebase investigator for Heiwa. Synthesizes context from code, docs, and logs without mutating state.
status: active
prompt_file: prompt.md
tags: [research, investigation, read-only]
tool_profile: read_only

targets:
  gemini:
    enabled: true
    output: .gemini/agents/heiwa-researcher.md
    model: auto-gemini-3
    max_turns: 15
  claude:
    enabled: true
    output: .claude/agents/heiwa-researcher.md
    model: sonnet
    max_turns: 15
  codex:
    enabled: true
    generated_dir: ops/agents/heiwa-researcher/generated/codex/heiwa-researcher
    install_name: heiwa-researcher
```

`ops/agents/heiwa-researcher/prompt.md`:

```markdown
# Heiwa Researcher Subagent

You are the **Heiwa Researcher**, a specialized scout designed to investigate the codebase, gather context, and analyze logs without modifying the state.

## Core Mandates

- **Read-Only Scope:** You are strictly forbidden from mutating code, committing changes, or deploying infrastructure. Use search tools and read operations to gather intelligence.
- **Context Synthesis:** Read deep into the `ops/rooms/` architecture docs, `docs/`, and `GEMINI.md` hard rules to provide holistic answers to queries.
- **Log Analysis:** Be prepared to parse telemetry and execution logs (redacting any accidental secrets) to diagnose issues or understand system behavior.
- **High-Signal Reporting:** Synthesize findings into concise, actionable summaries for the orchestrator or operator. Avoid noisy play-by-plays.

## Workflow

1. **Scout:** Use `grep_search` and `glob` to locate relevant systems.
2. **Read:** Pull targeted ranges of files to understand implementations and documentation.
3. **Analyze:** Cross-reference findings with the "Hard Rules" in `GEMINI.md`.
4. **Synthesize:** Present your findings clearly and concisely.
```

- [ ] **Step 6: Commit**

```bash
git add ops/agents/heiwa-architect/ ops/agents/heiwa-security/ ops/agents/heiwa-builder/ ops/agents/heiwa-operator/ ops/agents/heiwa-researcher/
git commit -m "feat: migrate five Heiwa specialists to canonical form"
```

---

### Task 3: TDD wrapper generators — banner, Gemini, Claude, Codex

**Files:**
- Create: `scripts/sync_agents.py`
- Create: `scripts/tests/test_sync_agents.py`
- Modify: `pyproject.toml`

- [ ] **Step 1: Add test infrastructure to pyproject.toml**

Add `"scripts/tests"` to `testpaths` and `"scripts"` to `pythonpath`:

```toml
[tool.pytest.ini_options]
testpaths = ["apps/heiwa_hub/tests", "apps/heiwa_trading/tests", "scripts/tests"]
pythonpath = ["apps/heiwa_trading/src", "scripts"]
python_files = "test_*.py"
python_functions = "test_*"
addopts = "--tb=short -q"
```

- [ ] **Step 2: Write failing tests for banner and all three generators**

Create `scripts/tests/test_sync_agents.py`:

```python
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
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `uv run pytest scripts/tests/test_sync_agents.py -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'sync_agents'`

- [ ] **Step 4: Create `scripts/sync_agents.py` with generators**

```python
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `uv run pytest scripts/tests/test_sync_agents.py -v`
Expected: All 19 tests PASS

- [ ] **Step 6: Commit**

```bash
git add scripts/sync_agents.py scripts/tests/test_sync_agents.py pyproject.toml
git commit -m "feat: TDD wrapper generators for Gemini, Claude, and Codex"
```

---

### Task 4: TDD registry loading + CLI generate mode

**Files:**
- Modify: `scripts/sync_agents.py`
- Modify: `scripts/tests/test_sync_agents.py`

- [ ] **Step 1: Write failing tests for registry loading**

Append to `scripts/tests/test_sync_agents.py`:

```python
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest scripts/tests/test_sync_agents.py::test_load_registry_returns_five_agents -v`
Expected: FAIL — `ImportError: cannot import name 'load_registry'`

- [ ] **Step 3: Implement registry loading and CLI**

Add to `scripts/sync_agents.py`:

```python
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest scripts/tests/test_sync_agents.py -v`
Expected: All 22 tests PASS

- [ ] **Step 5: Commit**

```bash
git add scripts/sync_agents.py scripts/tests/test_sync_agents.py
git commit -m "feat: registry loading and CLI generate mode"
```

---

### Task 5: Run sync + commit generated wrappers

**Files:**
- Generated: `.gemini/agents/heiwa-*.md` (5 files, replaced)
- Generated: `.claude/agents/heiwa-*.md` (5 files, new)
- Generated: `ops/agents/*/generated/codex/*/SKILL.md` (5 files, new)

- [ ] **Step 1: Run sync to generate all wrappers**

Run: `uv run scripts/sync_agents.py`
Expected output:
```
  Generated .gemini/agents/heiwa-architect.md
  Generated .claude/agents/heiwa-architect.md
  Generated ops/agents/heiwa-architect/generated/codex/heiwa-architect/SKILL.md
  Generated .gemini/agents/heiwa-security.md
  Generated .claude/agents/heiwa-security.md
  Generated ops/agents/heiwa-security/generated/codex/heiwa-security/SKILL.md
  Generated .gemini/agents/heiwa-builder.md
  Generated .claude/agents/heiwa-builder.md
  Generated ops/agents/heiwa-builder/generated/codex/heiwa-builder/SKILL.md
  Generated .gemini/agents/heiwa-operator.md
  Generated .claude/agents/heiwa-operator.md
  Generated ops/agents/heiwa-operator/generated/codex/heiwa-operator/SKILL.md
  Generated .gemini/agents/heiwa-researcher.md
  Generated .claude/agents/heiwa-researcher.md
  Generated ops/agents/heiwa-researcher/generated/codex/heiwa-researcher/SKILL.md
Done.
```

- [ ] **Step 2: Spot-check a generated Gemini wrapper**

Run: `head -10 .gemini/agents/heiwa-architect.md`
Expected: YAML frontmatter with `name: heiwa-architect`, followed by generated banner

- [ ] **Step 3: Spot-check a generated Claude wrapper**

Run: `head -12 .claude/agents/heiwa-researcher.md`
Expected: YAML frontmatter with `disallowedTools: ["Write", "Edit", "MultiEdit", "Bash"]`, followed by generated banner

- [ ] **Step 4: Spot-check a generated Codex wrapper**

Run: `head -12 ops/agents/heiwa-researcher/generated/codex/heiwa-researcher/SKILL.md`
Expected: YAML frontmatter, generated banner, then `## Read-Only Policy` section

- [ ] **Step 5: Commit generated wrappers**

```bash
git add .gemini/agents/ .claude/agents/ ops/agents/*/generated/
git commit -m "feat: generate cross-runtime wrappers from canonical sources"
```

---

### Task 6: TDD --check mode

**Files:**
- Modify: `scripts/sync_agents.py`
- Modify: `scripts/tests/test_sync_agents.py`

- [ ] **Step 1: Write failing tests for check mode**

Append to `scripts/tests/test_sync_agents.py`:

```python
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


def test_check_codex_config_current_state():
    """Codex config check should report missing surfaces before parity fix."""
    errors = check_codex_config()
    # Before Task 8 fixes the config, figma/notion/codebase-retrieval are missing
    missing_names = [e for e in errors if "figma" in e or "notion" in e or "codebase-retrieval" in e]
    assert len(missing_names) > 0


def test_check_claude_config_passes():
    """Claude config should already have enableAllProjectMcpServers."""
    errors = check_claude_config()
    mcp_errors = [e for e in errors if "enableAllProjectMcpServers" in e]
    assert mcp_errors == []


def test_check_gemini_config_passes():
    """Gemini config should already have required keys."""
    errors = check_gemini_config()
    assert errors == []
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest scripts/tests/test_sync_agents.py::test_check_wrapper_drift_clean_passes -v`
Expected: FAIL — `ImportError: cannot import name 'check_wrapper_drift'`

- [ ] **Step 3: Implement check mode**

Add to `scripts/sync_agents.py`:

```python
import tomllib

# -- Config parity constants --

REQUIRED_CODEX_MCP = {
    "MCP_DOCKER", "playwright", "railway", "figma", "notion", "codebase-retrieval",
}
REQUIRED_CODEX_PLUGINS = {"github", "cloudflare", "google-drive", "hugging-face"}
REQUIRED_CODEX_FEATURES = {"multi_agent", "guardian_approval", "prevent_idle_sleep"}


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

    security = config.get("security", {})
    if not security.get("environmentVariableRedaction", {}).get("enabled"):
        errors.append("GEMINI CONFIG: environmentVariableRedaction not enabled")

    filtering = config.get("context", {}).get("fileFiltering", {})
    if not filtering.get("respectGitIgnore"):
        errors.append("GEMINI CONFIG: respectGitIgnore not enabled")

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
```

Update the `main()` function's check branch:

```python
    if args.check:
        ok = cmd_check(agents)
        return 0 if ok else 1
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest scripts/tests/test_sync_agents.py -v`
Expected: All 26 tests PASS

- [ ] **Step 5: Run --check to verify current state**

Run: `uv run scripts/sync_agents.py --check`
Expected: FAIL with Codex config parity errors (figma, notion, codebase-retrieval missing — this is correct; fixed in Task 8)

- [ ] **Step 6: Commit**

```bash
git add scripts/sync_agents.py scripts/tests/test_sync_agents.py
git commit -m "feat: --check mode with drift, orphan, and config parity"
```

---

### Task 7: TDD --install-codex mode

**Files:**
- Modify: `scripts/sync_agents.py`
- Modify: `scripts/tests/test_sync_agents.py`

- [ ] **Step 1: Write failing test for install-codex**

Append to `scripts/tests/test_sync_agents.py`:

```python
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest scripts/tests/test_sync_agents.py::test_install_codex_creates_symlinks -v`
Expected: FAIL — `ImportError: cannot import name 'cmd_install_codex'`

- [ ] **Step 3: Implement --install-codex**

Add to `scripts/sync_agents.py`:

```python
import shutil

DEFAULT_SKILLS_DIR = Path.home() / ".agents" / "skills"


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
```

Update `main()`:

```python
    if args.install_codex:
        cmd_install_codex(agents, copy_mode=args.copy)
        print("Done.")
        return 0
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest scripts/tests/test_sync_agents.py -v`
Expected: All 28 tests PASS

- [ ] **Step 5: Commit**

```bash
git add scripts/sync_agents.py scripts/tests/test_sync_agents.py
git commit -m "feat: --install-codex with symlink and copy modes"
```

---

### Task 8: Close config parity gaps + final verification

**Files:**
- Modify: `.codex/config.toml`
- Modify: `scripts/tests/test_sync_agents.py`

- [ ] **Step 1: Add missing MCP servers and plugins to Codex project config**

Update `.codex/config.toml` — add these sections after the existing `[mcp_servers.playwright]` block:

```toml
[mcp_servers.figma]
url = "https://mcp.figma.com/mcp"

[mcp_servers.notion]
url = "https://mcp.notion.com/mcp"

[mcp_servers.codebase-retrieval]
command = "auggie"
args = ["--mcp", "--mcp-auto-workspace"]
```

Add these after the existing `[plugins."cloudflare@openai-curated"]` block:

```toml
[plugins."google-drive@openai-curated"]
enabled = true

[plugins."hugging-face@openai-curated"]
enabled = true
```

- [ ] **Step 2: Update test to expect clean Codex config check**

Replace `test_check_codex_config_current_state` in the test file:

```python
def test_check_codex_config_passes_after_parity_fix():
    """After config parity fix, Codex config check should pass clean."""
    errors = check_codex_config()
    assert errors == [], f"Unexpected Codex config errors: {errors}"
```

- [ ] **Step 3: Run all tests**

Run: `uv run pytest scripts/tests/test_sync_agents.py -v`
Expected: All tests PASS

- [ ] **Step 4: Run full --check**

Run: `uv run scripts/sync_agents.py --check`
Expected: `All checks passed.`

- [ ] **Step 5: Run --install-codex**

Run: `uv run scripts/sync_agents.py --install-codex`
Expected: Five symlinks created in `~/.agents/skills/`

- [ ] **Step 6: Verify symlinks resolve**

Run: `ls -la ~/.agents/skills/heiwa-architect/SKILL.md`
Expected: Symlink target resolves to `ops/agents/heiwa-architect/generated/codex/heiwa-architect/SKILL.md`

- [ ] **Step 7: Commit**

```bash
git add .codex/config.toml scripts/tests/test_sync_agents.py
git commit -m "feat: close Codex config parity gaps, all checks pass"
```

- [ ] **Step 8: Run full test suite**

Run: `uv run pytest -v`
Expected: All tests across the repo PASS (including the new sync_agents tests)

---

## Success Criteria Checklist

After all tasks are complete, verify:

1. `ops/agents/` contains five canonical agent folders with `agent.yaml` + `prompt.md`
2. `.gemini/agents/` contains five generated wrappers (all start with YAML frontmatter + generated banner)
3. `.claude/agents/` contains five generated wrappers (researcher includes `disallowedTools`)
4. `ops/agents/*/generated/codex/*/SKILL.md` contains five generated wrappers (researcher includes read-only policy)
5. `uv run scripts/sync_agents.py --check` exits 0
6. `~/.agents/skills/heiwa-*` symlinks resolve to generated Codex wrappers
7. `.codex/config.toml` declares all required MCP servers, plugins, and features
8. No hand-edited content remains in `.gemini/agents/` or `.claude/agents/` — all files are generated
