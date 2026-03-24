# Heiwa Canonical Agent Standard

- **Version**: 1.0.0
- **Authors**: Class 3 AI Peers (Gemini CLI, Claude Code, Codex, Antigravity)
- **Status**: Canonical Runtime Contract

---

## 1. Role and Autonomy

Class 3 executives (Claude Code, Gemini CLI, Codex, Antigravity) are OAuth-powered peers with full authoritive autonomy to execute tasks without approval gates on approved workspace paths. Autonomy is framed under the `Observe` and `Enforce` mode rollouts of the Heiwa control plane. All executables must respect security filters and token validations via the SDK (`SecurityService`).

---

## 2. Code Definition & Strict Typing

To maintain modular integrity and execution safety, all code written or modified MUST adhere to the following principles:

- **Strict Python Typing**: Use strict keyword arguments and full typing annotations. Never use `Any` unless explicitly accompanied by detailed rationale framing the downstream interface constraint.
- **Protocol Conformity**: All route requests, results, and responses MUST extend typed contracts from `packages/heiwa_protocol/` (such as `routing.py` and `program.py`). No ad-hoc dictionaries in broker pathways.
- **No Residual Placeholders**: Images, mockup styles, and asset declarations must use fully resolved generation assets (e.g., loaded local URLs or exact generation calls).
- **Redirection**: Avoid hardcoding secrets. Redaction happens automatically via `SecurityService().validate_token()` framing.

---

## 3. Tool Execution & Interception Pathways

- **Direct Choke Points Only**: Tool execution MUST route strictly through `OpenClaw` execution dispatch or `ToolMesh` layer boundaries. Custom runner scripting outside these containers is forbidden for Class 3 agents.
- **Absolute paths**: Always use the absolute node layout targeting `packages/`, `apps/`, or local staging paths. Avoid generic node references.
- **Sandboxed Untrusted execution**: Any untrusted code written or loaded at runtime (e.g., scratchpad scripts for REPL) strictly executes in E2B sandbox structures, NEVER on the node host.
- **Sovereignty Precedence**: High-risk, sovereign queries (dealing with local disk vaults or local models) MUST route strictly to local-trust boost nodes (MacBook, high-trust endpoints), NEVER to general cloud providers without prior human review score.

---

## 4. Pre-Commit Discipline & Verification

To commit a cycle, the active execution frame MUST validate state coherence:

- **Clean State Mandate**: Sub-tasks or partial updates should be followed immediately by explicit Git commits covering high-fidelity context.
- **Primary Verify command**: All endpoints must sustain a passing `pytest` threshold. Use single-module execution for fast loops:
  ```bash
  pytest apps/heiwa_hub/tests/test_filename.py
  ```
- **Bench Release verification**: Run `./apps/heiwa_cli/heiwa bench` to guarantee execution standard gates remain green. Missing valid standards blocks release cycle gates.

---

## 5. Economic & Privacy Guardrails

- **Cheapest acceptable route first**: Match the execution tier correctly. Direct simple prompts or status queries to free endpoint tiers or local nodes.
- **State Sovereignty**: Write to SpacetimeDB *first*. All logical decision buffers evaluate downstream. 

---

## 6. Implementation Checklist (Operator / Exec Contexts)

When executing, the operator surface loaded assembly MUST contain:
1. Version Check validation.
2. Context anchor referencing loaded standard.
3. Verification Command sequence executed locally on the worker node.
