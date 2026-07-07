# Design Spec: Cross-Runtime Agent Canonicalization

**Date:** 2026-03-29
**Status:** Approved
**Gate:** Required before shared Heiwa sub-agent rollout across Claude, Codex, and Gemini

## Problem

Heiwa currently has repo-local Gemini agent wrappers in `.gemini/agents/`, but no equivalent canonical source for cross-runtime specialists. Claude and Codex behavior depend on separate runtime surfaces, and Codex in particular discovers reusable specialists from the machine-global `~/.agents/skills/` path rather than a repo-local agent directory.

This creates four problems:

- Heiwa specialist definitions drift across tools because there is no single authoring surface
- Repo behavior is not portable; Heiwa still depends on Devon's machine-global config for parts of Codex and Claude runtime behavior
- Generated or copied wrappers can become stale or orphaned without a single drift gate
- Heiwa automations and Heiwa specialists are both operator assets, but they are different systems and need different canonical homes

## Goal

Create one repo-local canonical source for Heiwa shared specialists, generate committed runtime wrappers for Gemini and Claude, generate installable Codex skill wrappers, and make Heiwa project configs portable-first without re-declaring Devon-global preferences that are not Heiwa requirements.

## Scope

### Delivery Boundary

This design defines one immediate implementation target and one deferred follow-on:

- **Immediate implementation target:** canonicalize the five shared Heiwa specialists, generate runtime wrappers, install Codex wrappers into the real discovery path, and close project runtime parity gaps required for those specialists to work portably
- **Deferred follow-on:** create a parallel canonical registry for Heiwa automations under `ops/automations/`

The first implementation plan derived from this spec should cover the immediate implementation target only. The automation registry is documented here for boundary clarity, but it is not part of the first execution plan.

### Initial Shared Specialists

The initial canonical migration includes all five existing Heiwa Gemini specialists:

- `heiwa-architect`
- `heiwa-security`
- `heiwa-builder`
- `heiwa-operator`
- `heiwa-researcher`

Authoritative migration source files:

- `.gemini/agents/heiwa-architect.md`
- `.gemini/agents/heiwa-security.md`
- `.gemini/agents/heiwa-builder.md`
- `.gemini/agents/heiwa-operator.md`
- `.gemini/agents/heiwa-researcher.md`

### Portable Runtime Targets

- **Gemini:** repo-local wrappers in `.gemini/agents/`
- **Claude:** repo-local wrappers in `.claude/agents/`
- **Codex:** generated skill wrappers installed into `~/.agents/skills/` via an explicit sync/install step

### Related Portability Work

- Heiwa project config parity for Codex, Claude, and Gemini
- Deferred canonical automation registry under `ops/automations/`

### Out of Scope

- Vendor-curated shared skills
- Devon-global non-Heiwa skills
- Third-party extension agents (for example Gemini superpowers agents)
- Automatic promotion of existing Heiwa Codex-only skills into the shared registry

## Canonical Architecture

### 1. Single Authoring Surface

All shared Heiwa specialists are authored only under `ops/agents/`:

```text
ops/agents/
  README.md
  registry.yaml

  heiwa-architect/
    agent.yaml
    prompt.md

  heiwa-security/
    agent.yaml
    prompt.md
```

Contract:

- `registry.yaml` is the catalog of canonical agents and managed targets
- `agent.yaml` is the structured manifest for one specialist
- `prompt.md` is the canonical prompt body for one specialist
- no human edits are made directly in `.gemini/agents/`, `.claude/agents/`, or generated Codex wrappers

### 2. Per-Agent Canonical Files

Each canonical agent folder contains:

#### `agent.yaml`

Portable metadata plus minimal runtime overrides:

```yaml
id: heiwa-architect
name: Heiwa Architect
description: Specialized architect for Heiwa state, mesh, and protocol changes.
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
  codex:
    enabled: true
    generated_dir: ops/agents/heiwa-architect/generated/codex/heiwa-architect
    install_name: heiwa-architect
    model: gpt-5.4-mini
```

Recommended fields:

- `id`
- `name`
- `description`
- `status`
- `prompt_file`
- `tags`
- `tool_profile`
- `targets`

#### `prompt.md`

The canonical instruction body for the specialist. This is the only place humans edit specialist prompt text.

### 3. Generated Runtime Surfaces

Generated outputs are committed for reviewability, but they are never hand-authored.

#### Gemini

Managed wrappers live in:

- `.gemini/agents/<id>.md`

#### Claude

Managed wrappers live in:

- `.claude/agents/<id>.md`

#### Codex

Managed generated wrappers live alongside the canonical agent:

- `ops/agents/<id>/generated/codex/<install_name>/SKILL.md`

Codex does not treat this repo path as native discovery. It is only the repo-owned generated source for installation into the real discovery path.

### 4. Runtime Wrapper Mapping

The generator must emit explicit runtime-native files from the canonical manifest.

#### Gemini wrapper example

```md
---
name: heiwa-architect
description: Specialized architect for Heiwa state, mesh, and protocol changes.
tools: ["*"]
model: auto-gemini-3
max_turns: 15
---

[generated banner]

[contents of prompt.md]
```

Gemini field mapping:

- `agent.yaml.id` → `name`
- `agent.yaml.description` → `description`
- `targets.gemini.model` → `model`
- `targets.gemini.max_turns` → `max_turns`
- canonical `tool_profile` → Gemini `tools`
- `prompt.md` body → wrapper markdown body after the generated banner

#### Claude wrapper example

```md
---
name: heiwa-architect
description: Specialized architect for Heiwa state, mesh, and protocol changes.
model: sonnet
maxTurns: 15
---

[generated banner]

[contents of prompt.md]
```

Claude field mapping:

- `agent.yaml.id` → `name`
- `agent.yaml.description` → `description`
- `targets.claude.model` → `model`
- `targets.claude.max_turns` → `maxTurns`
- canonical `tool_profile` → Claude `disallowedTools` when the agent is restrictive
- `prompt.md` body → wrapper markdown body after the generated banner

For restrictive agents such as `heiwa-researcher`, the generator must emit Claude `disallowedTools` derived from the canonical `tool_profile`. That field is part of the generated wrapper contract and must never be hand-authored in `.claude/agents/`.

#### Codex wrapper example

```md
---
name: heiwa-architect
description: Specialized architect for Heiwa state, mesh, and protocol changes.
---

[generated banner]

[contents of prompt.md]
```

Codex field mapping:

- `agent.yaml.id` → `name`
- `agent.yaml.description` → `description`
- `prompt.md` body → `SKILL.md` body after the generated banner

Codex skill files do not carry a runtime model override in the same way Gemini and Claude wrappers do. The Codex model hint remains canonical metadata in `agent.yaml` for installation and future runtime use, but is not required in generated `SKILL.md` frontmatter.

### 5. Generated Wrapper Contract

Every generated wrapper must:

- start with a generated banner
- reference its canonical sources under `ops/agents/<id>/agent.yaml` and `ops/agents/<id>/prompt.md`
- declare that manual edits are forbidden

The sync tool fully owns managed output directories.

## Sync, Install, and Drift Control

### 1. Sync Command

One repo command owns generation and verification:

```bash
uv run scripts/sync_agents.py
```

Behavior:

1. read `ops/agents/registry.yaml`
2. load each canonical `agent.yaml` and `prompt.md`
3. regenerate Gemini wrappers in `.gemini/agents/`
4. regenerate Claude wrappers in `.claude/agents/`
5. regenerate Codex wrappers under `ops/agents/<id>/generated/codex/`

### 2. Codex Install Bridge

Codex native discovery remains global on this Mac via `~/.agents/skills/`.

Optional install step:

```bash
uv run scripts/sync_agents.py --install-codex
```

Default behavior on Devon's Mac:

- install by symlink from `ops/agents/<id>/generated/codex/<install_name>/` to `~/.agents/skills/<install_name>/`

Alternate behavior:

- `--copy` mode for environments where symlinks are not desired

### 3. Check Mode

One verification gate must cover the full system:

```bash
uv run scripts/sync_agents.py --check
```

This command fails if any of the following are false:

1. generated Gemini wrappers are current
2. generated Claude wrappers are current
3. generated Codex wrappers are current
4. there are no orphan wrappers in `.gemini/agents/`
5. there are no orphan wrappers in `.claude/agents/`
6. there are no orphan generated Codex wrappers under `ops/agents/*/generated/codex/`
7. required Codex installs are present and point at the expected generated source
8. Heiwa project runtime config remains self-sufficient for repo-required features, MCP servers, and plugins

This is the future CI candidate. The validation surface is one command, not a manual checklist.

#### `--check` Runtime Matrix

The checker must implement explicit pass/fail assertions per runtime:

| Runtime       | Required assertions                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Gemini        | wrapper exists for every enabled Gemini target; no orphan `.gemini/agents/*.md`; frontmatter `name`, `description`, `model`, `max_turns`, and `tools` match canonical manifest; wrapper body matches generated banner + canonical `prompt.md`                                                                                                                                                                                                                                                                                                                                  |
| Claude        | wrapper exists for every enabled Claude target; no orphan `.claude/agents/*.md`; frontmatter `name`, `description`, `model`, and `maxTurns` match canonical manifest; restrictive agents include the generated Claude-native tool restriction field; wrapper body matches generated banner + canonical `prompt.md`                                                                                                                                                                                                                                                             |
| Codex         | generated `SKILL.md` exists for every enabled Codex target; no orphan generated Codex wrapper trees; `SKILL.md` frontmatter `name` and `description` match canonical manifest; generated banner references canonical source; required install target exists and either resolves to the expected generated wrapper when symlink-installed or is byte-equal to the generated wrapper when copy-installed                                                                                                                                                                         |
| Config parity | `.codex/config.toml` declares all Heiwa-required MCP servers (`MCP_DOCKER`, `playwright`, `railway`, `figma`, `notion`, `codebase-retrieval`), plugins (`github`, `cloudflare`, `google-drive`, `hugging-face`), and features (`multi_agent`, `guardian_approval`, `prevent_idle_sleep`); `.claude/settings.json` exists and contains `enableAllProjectMcpServers: true` plus all Heiwa-required `enabledPlugins`; `.gemini/settings.json` exists and contains `defaultApprovalMode`, `environmentVariableRedaction.enabled: true`, and `fileFiltering.respectGitIgnore: true` |

## Runtime Parity and Config Layering

### 1. Portable-First Rule

Heiwa project config must be sufficient for work in this repo even if Devon's global config disappears. The test is:

> If Devon's global config disappeared, would Heiwa's project config alone supply everything needed to work in this repo?

The answer must be **yes** for:

- required MCP servers
- required plugins
- required runtime features

The answer does **not** need to be yes for:

- Devon-global model preferences
- Devon-global approval policy preferences
- Devon-global sandbox preferences

### 2. Additive Config Layering

When a runtime supports inheritance, project config should be additive, not duplicative.

That means:

- project config declares what Heiwa needs
- project config does not copy non-Heiwa global defaults such as `model = "gpt-5.4"` or `sandbox_mode`

### 3. Codex Parity Contract

`/Users/dmcgregsauce/heiwa/.codex/config.toml` must explicitly declare the Heiwa-required surfaces currently inherited from Devon's machine-global Codex config.

Required Heiwa project-level Codex declarations:

- MCP servers: `MCP_DOCKER`, `playwright`, `railway`, `figma`, `notion`, `codebase-retrieval`
- plugins: `github`, `cloudflare`, `google-drive`, `hugging-face`
- features: `multi_agent`, `guardian_approval`, `prevent_idle_sleep`

Non-goals for project Codex config:

- duplicating global `model`
- duplicating global `approval_policy`
- duplicating global `sandbox_mode`

### 4. Claude Parity Contract

Claude discovers project agents natively from `.claude/agents/*.md` — no install bridge needed.

`.claude/settings.json` is the repo-authoritative Heiwa configuration surface. Required keys:

- `enableAllProjectMcpServers: true` — ensures all project-declared MCP servers are available
- `enabledPlugins` — declares Heiwa-required Claude plugins by name
- `worktree.symlinkDirectories` — repo-specific worktree behavior (e.g., `.venv`)
- `permissions` — repo-level permission defaults

Claude agent wrapper frontmatter fields (emitted by the generator):

- `name` (string, required) — maps from `agent.yaml.id`
- `description` (string, required) — maps from `agent.yaml.description`
- `model` (string, optional) — maps from `targets.claude.model`; valid values: `opus`, `sonnet`, `haiku`
- `maxTurns` (integer, optional) — maps from `targets.claude.max_turns`
- `disallowedTools` (string array, optional) — emitted only for restrictive agents; maps from `tool_profile`

Heiwa must not require:

- secrets committed to project config
- machine-global Claude agent files for core Heiwa specialists
- user-level `~/.claude/settings.json` for any Heiwa-specific behavior

### 5. Gemini Parity Contract

`/Users/dmcgregsauce/heiwa/.gemini/settings.json` remains the project authority for:

- project agent wrappers in `.gemini/agents/`
- repo-local filtering and context behavior
- repo-local safety settings relevant to Heiwa work

## Migration Strategy

### 1. Shared Specialist Migration

Initial migration is exactly the five existing Heiwa Gemini specialists sourced from the current repo-local Gemini wrappers:

- `heiwa-architect`
- `heiwa-security`
- `heiwa-builder`
- `heiwa-operator`
- `heiwa-researcher`

Each is migrated into:

- `ops/agents/<id>/agent.yaml`
- `ops/agents/<id>/prompt.md`
- generated Gemini wrapper
- generated Claude wrapper
- generated Codex wrapper

### 2. Codex Skill Promotion Rule

Existing Heiwa Codex-only skills are **not** automatically promoted into the shared registry.

Promotion is manual and intentional:

- if something is a reusable specialist identity, it may be promoted into `ops/agents/`
- if something is workflow guidance, process glue, or an operational playbook, it remains a skill or automation

This prevents scope creep and preserves the distinction between identity and workflow.

## Automation Boundary

Automations are a sibling system, not agent folders. This is a deferred follow-on, not part of the first implementation plan.

Canonical home:

```text
ops/automations/
  registry.yaml
  heiwa-operator-pentest/
    automation.yaml
    prompt.md
```

Rules:

- `ops/agents/` defines specialist identities
- `ops/automations/` defines recurring tasks
- an automation may reference an agent id, but it is not itself an agent
- automation sync/install is separate from agent sync/install

## Success Criteria

The immediate implementation target is complete when all of the following are true:

1. all five shared Heiwa specialists are authored only in `ops/agents/`
2. `.gemini/agents/` and `.claude/agents/` are fully generated and contain no orphans
3. Codex generated wrappers exist under `ops/agents/*/generated/codex/` and can be installed into `~/.agents/skills/`
4. Heiwa project runtime configs are portable-first and additive
5. one command, `uv run scripts/sync_agents.py --check`, verifies drift, orphan detection, Codex install state, and project config self-sufficiency

Deferred follow-on success criteria:

1. automations have an explicit canonical home outside the agent tree
2. agent sync/install and automation sync/install remain separate workflows

## Implementation Sequence

### Phase 1: Shared Agent Canonicalization

1. create `ops/agents/README.md` and `ops/agents/registry.yaml`
2. migrate the five existing Heiwa Gemini agents into canonical `agent.yaml` + `prompt.md` folders
3. implement `scripts/sync_agents.py` generation for Gemini, Claude, and Codex
4. add Codex install/check support

### Phase 2: Runtime Parity

1. close project runtime parity gaps in `.codex/config.toml`, `.claude/settings.json`, and `.gemini/settings.json`
2. make `uv run scripts/sync_agents.py --check` enforce the runtime matrix defined above

### Phase 3: Deferred Follow-On

1. create `ops/automations/` as the canonical automation system
2. evaluate any existing Heiwa Codex-only skills for manual promotion into the shared registry

## Security and Hygiene

- generated wrappers must never contain secrets
- project config must not commit machine-global credentials
- generated outputs are reviewable artifacts, not authoring surfaces
- orphan and drift detection are mandatory because silent wrapper divergence is a security and reliability risk

## Schema Appendix

### Canonical `tool_profile`

Allowed v1 values:

- `full_access`
- `read_only`

Translation rules:

| `tool_profile` | Gemini emit                                                                          | Claude emit                                               | Codex emit                                                                                     |
| -------------- | ------------------------------------------------------------------------------------ | --------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `full_access`  | `tools: ["*"]`                                                                       | omit `disallowedTools`                                    | no wrapper-level restriction; rely on prompt contract                                          |
| `read_only`    | `tools: ["read_file", "grep_search", "glob", "list_directory", "google_web_search"]` | `disallowedTools: ["Write", "Edit", "MultiEdit", "Bash"]` | no wrapper-level restriction; generator adds a read-only policy section to the `SKILL.md` body |

In v1, `heiwa-researcher` is the only restrictive shared specialist and uses `tool_profile: read_only`. The other four shared specialists use `tool_profile: full_access`.

### Generated Banner Format

Every generated wrapper must start with a short banner that includes:

- `GENERATED FILE - DO NOT EDIT`
- canonical manifest path
- canonical prompt path
- regeneration command: `uv run scripts/sync_agents.py`

Example banner:

```md
<!-- GENERATED FILE - DO NOT EDIT
manifest: ops/agents/heiwa-architect/agent.yaml
prompt: ops/agents/heiwa-architect/prompt.md
regen: uv run scripts/sync_agents.py
-->
```
