# Heiwa Canonical Agent Standard

- **Version**: 1.2.0
- **Authors**: Class 3 AI Peers (Gemini CLI, Claude Code, Codex, Antigravity)
- **Status**: Canonical Runtime Contract

---

## 1. Role and Autonomy

Class 3 executives (Claude Code, Gemini CLI, Codex, Antigravity) are OAuth-powered peer executors over the same Heiwa stack. The authority model is:

- **Local JSONL evidence** = canonical durable execution truth
- **Lance** = derived, rebuildable local recall index
- **Rust core** = execution authority
- **Provider session** = operator client and orchestration owner
- **Python** = bounded legacy bridge only
- **TypeScript** = package/web surface

Routine subagent lifecycle work stays provider-owned. Human escalation is reserved for destructive host actions, irreversible external side effects, credential or policy break-glass, or platform/harness prompts that cannot be suppressed by configuration.

---

## 2. Code Definition & Strict Typing

To maintain modular integrity and execution safety, all code written or modified MUST adhere to the following principles:

- **Strict Python Typing**: Use strict keyword arguments and full typing annotations. Never use `Any` unless explicitly accompanied by detailed rationale framing the downstream interface constraint.
- **Protocol Conformity**: All route requests, results, and responses MUST extend typed contracts from `packages/heiwa_protocol/` (such as `routing.py` and `program.py`). No ad-hoc dictionaries in broker pathways.
- **No Residual Placeholders**: Images, mockup styles, and asset declarations must use fully resolved generation assets (e.g., loaded local URLs or exact generation calls).
- **Redirection**: Avoid hardcoding secrets. Redaction happens automatically via `SecurityService().validate_token()` framing.

---

## 3. Tool Execution & Interception Pathways

- **Provider-native first**: Native provider tools, plugins, MCP servers, and specialist wrappers remain enabled. Heiwa adds boot order, policy, and cross-runtime specialists; it does not replace provider-native capabilities.
- **Project control surfaces**: Repo-local provider posture lives in `.codex/`, `.claude/`, and `.gemini/`. Canonical specialists live in `ops/agents/` and sync into `.gemini/agents/`, `.claude/agents/`, and `~/.codex/skills`.
- **Sandboxed untrusted execution**: Any untrusted code written or loaded at runtime strictly executes in E2B or another explicit sandbox boundary, never on the host by default.
- **Sovereignty precedence**: High-risk sovereign work (local disk vaults, local models, private operator state) routes to trusted boost nodes, not general cloud providers.

---

## 4. Pre-Commit Discipline & Verification

To close a cycle, the active execution frame must validate current truth instead of relying on stale defaults:

- **Clean state mandate**: Sub-tasks or partial updates should be followed by explicit git commits or a deliberate staged checkpoint.
- **Primary verification**: Run the narrowest affected test or lint command first, then the broader repo gate if the narrow command passes.
- **Local agent baseline**: Final repo-health handoffs must run the local-only baseline gate:
  ```bash
  bash scripts/check_agent_baseline.sh
  ```
- **Runtime baseline verification**: Repo and operator checks should stay green:
  ```bash
  bash scripts/check_runtime_baseline.sh
  bash scripts/check_heiwa_core_dockerfile.sh
  bash scripts/check_release_metadata.sh
  bash scripts/audit_product_surface.sh
  ```
- **Remote pre-flight separation**: Remote health is not implied by local green checks. `git fetch`, `git pull`, `git push`, `gh run`, `gh release`, `wrangler deploy`, and equivalent network-promotion operations require explicit assignment and the checklist in `docs/agent-baseline-workflow.md`.
- **Vendor quarantine**: Untracked `vendor/oss-lifts` is reference material only until Devon assigns a vendor-policy slice. Do not add, delete, import from, or cite it as product evidence.

---

## 5. Economic & Privacy Guardrails

- **Cheapest acceptable route first**: Match the execution tier correctly. Direct simple prompts or status queries to free endpoint tiers or local nodes.
- **State Sovereignty**: Append canonical evidence locally first. Derived Lance indexes and future redacted sync evaluate downstream.

---

## 6. Implementation Checklist (Operator / Exec Contexts)

When executing, the loaded operator surface must contain:

1. Version check validation.
2. Context anchors loaded from `HEIWA.md`, `AGENTS.md`, `docs/local-self-operation.md`, and the relevant room files.
3. Provider-local config active for the current repo.
4. Verification command sequence executed locally on the worker or operator node.
