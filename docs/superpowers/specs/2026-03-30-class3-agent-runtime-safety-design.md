# Design Spec: Class 3 Agent Runtime Safety

**Date:** 2026-03-30
**Status:** Approved
**Gate:** Required before Phase 1 live rollout of safety controls across Claude, Codex, Gemini, and Antigravity

## Problem

Devon's Class 3 executive agents currently operate with uneven runtime controls.

- Gemini already has active native hooks, but only for a narrow shell-focused check and date injection.
- Claude has a hook-capable runtime surface but no custom safety policy wired into it.
- Codex exposes strong config, sandbox, approval, plugin, and instruction controls, but the current local install does not expose a native hook surface comparable to Claude or Gemini.
- Antigravity appears to be extension-driven rather than hook-driven, so safety must be implemented at its real execution layer rather than by pretending it supports Claude-style hook events.

This creates three immediate risks:

1. Safety behavior drifts across agents because each runtime is hardened differently or not at all.
2. Destructive shell, network, and file mutation actions are not consistently fail-closed.
3. Observability is fragmented, so it is hard to tell what was blocked, what ran under override, and what safety surface was actually active.

## Goal

Implement Phase 1 runtime safety across all four Class 3 executive agents using each tool's native or real execution control plane, while preserving one shared operating model:

- fail-closed defaults
- hybrid bypass lease for emergencies
- coverage for shell, outbound network, and file mutation surfaces where the runtime exposes them
- local auditability per agent
- no false claims of cross-agent hook parity

## Scope

### In Scope

- Phase 1A `Safety` only
- Live rollout across Gemini, Claude, Codex, and Antigravity in the same phase
- Native per-agent enforcement paths
- Short-lived emergency bypass leases
- Local audit logging for deny, allow-under-lease, timeout, and policy error outcomes
- Verification using simulated benign and malicious actions per agent

### Ordered Follow-On Stages

The user-prioritized stage order after Safety is:

1. `Safety`
2. `Context bootstrap`
3. `Observability`
4. `Control/routing`

This spec defines the Safety stage only, but it preserves room for the later stages without requiring a redesign.

### Out of Scope

- Full cross-agent behavioral parity
- Shared supervisor/proxy architecture for all four tools
- Centralized shared runtime package for policy execution
- Repo-wide Heiwa routing changes
- Non-Class-3 providers
- Context/bootstrap behavior beyond what Safety needs for lease identity and auditability

## Design Decisions

### 1. Native Per-Agent Policy Stack

Phase 1 uses a native per-agent policy stack rather than a shared sidecar package or external supervisor.

Reasoning:

- It matches the real surfaces available in each runtime.
- It avoids building a fragile abstraction that hides capability differences.
- It ships faster because Gemini and Claude can use their hook systems directly, while Codex and Antigravity can be hardened through their actual runtime choke points.

### 2. Per-Agent Implementations, Not Shared Runtime Code

Safety logic is implemented per agent in the directories and formats that each runtime expects.

Reasoning:

- The user explicitly prefers per-agent implementations because config expectations differ.
- Native configs are easier to maintain and less likely to break on runtime updates.
- Audit trails and lease state stay close to the runtime that owns them.

### 3. Hybrid Lease Bypass

The override model is a hybrid lease:

- default behavior is fail-closed
- risky actions are blocked unless explicitly allowed
- a temporary bypass lease can allow narrow classes of risky actions for a short time

Reasoning:

- Per-command-only override is safest but too slow for emergencies.
- Session-wide bypass is fast but creates too much blast radius.
- A short-lived lease with explicit reason and expiry provides the best balance.

## Common Policy Model

All four agents must implement the same policy semantics even when the enforcement mechanisms differ.

### Default Enforcement

- Policy evaluation is `fail-closed`.
- A matched dangerous action is denied unless an active lease explicitly covers it.
- A checker crash, timeout, malformed payload, or missing required metadata is treated as a denial.

### Protected Action Classes

- Destructive shell operations
- Outbound network actions that can exfiltrate data or mutate remote state
- File writes and edits, including multi-edit and destructive rename/move flows
- Sensitive path access involving agent configs, auth material, shell startup files, SSH material, cloud credentials, and similar operator-critical locations

### Decision Outcomes

- `ALLOW`
- `BLOCK`
- `ALLOW_UNDER_LEASE`

Every agent must expose these outcomes in its local audit trail even if the native runtime does not use the same internal names.

## Per-Agent Architecture

### Gemini CLI

Primary control plane:

- native `BeforeTool` hooks
- native `BeforeAgent` hooks
- local config in `~/.gemini/settings.json`

Phase 1 implementation:

- expand the current shell-focused checker into a broader policy evaluator
- inspect tool invocations for shell commands, network-capable actions, and file mutation targets
- keep bypass lease state and audit logs under `~/.gemini/`

Expected result:

- Gemini remains genuinely hook-driven
- Safety is enforced before risky actions run
- Current hook usage becomes the first production implementation of the common policy model

### Claude Code

Primary control plane:

- native hook events such as `SessionStart`, `PreToolUse`, and post-tool events
- local config in `~/.claude/settings.json` and machine-local overrides in `~/.claude/settings.local.json`

Phase 1 implementation:

- add `PreToolUse` safety checks for shell, write/edit, and network/web surfaces
- reserve `SessionStart` for later bootstrap work, but keep config layout compatible with that next step
- write local lease state and audit logs under `~/.claude/`

Expected result:

- Claude becomes the strongest native hook implementation
- high-impact tools are checked before execution
- the hook model stays local to Claude rather than being forced into another runtime's shape

### Codex

Primary control plane:

- `~/.codex/config.toml`
- instruction layers such as `AGENTS.md`
- sandbox and approval settings
- optional launch/runtime wrappers if stricter enforcement is required than config alone can provide

Constraint:

Inspection of the current local Codex CLI install and exposed config surface did not reveal a native hook mechanism comparable to Claude or Gemini. This design treats that as a real capability boundary rather than an implementation gap to hide.

Phase 1 implementation:

- tighten config defaults and instruction posture
- use the real choke points Codex exposes: approval policy, sandboxing, plugin/MCP configuration, and any supported launch wrapper path
- if config and instruction hardening cannot actually mediate risky network or file-mutation actions, a Codex-specific wrapper becomes mandatory for Phase 1 completion
- only add lease behavior where there is an actual enforcement path

Expected result:

- Codex is hardened honestly through the controls it really has
- documentation and runtime posture do not imply hook parity that does not exist

### Antigravity

Primary control plane:

- extension/runtime environment under `~/.antigravity/`
- the installed Devon operator extension
- VS Code-compatible settings or runtime bridges only where they actually broker execution

Constraint:

The local Antigravity footprint appears extension-driven rather than hook-driven.

Phase 1 implementation:

- identify the real execution broker
- apply safety at that layer rather than at passive editor settings
- store local lease state and audit trail in Antigravity-owned runtime state
- if no reliable execution broker exists, Phase 1 must stop short of claiming Antigravity parity rather than silently downgrade the guarantee

Expected result:

- Antigravity safety is implemented where commands or tool actions are actually mediated
- the design avoids fake generic hook plumbing

## Safety Policy Behavior

### Blocking Rules

The policy engine for each agent must block at least the following by default unless explicitly trusted or covered by an active lease:

- recursive deletion or overwrite patterns
- destructive file operations against sensitive paths
- shell commands with obvious credential exfiltration or host tampering intent
- arbitrary outbound POST/PUT/PATCH/DELETE operations
- unscoped remote fetch or submit actions that can leak local content
- file mutation requests against agent configs, shell startup files, SSH material, cloud credentials, or similar operator-critical assets

### Lease Rules

Lease requirements:

- short TTL
- explicit reason string
- visible active state
- automatic expiry
- narrow scope where the runtime supports it

Preferred scope dimensions:

- action class
- optional path constraints
- optional domain or remote-target constraints

Blanket disable modes are discouraged and should only exist if the runtime makes narrower scoping impossible.

### Failure Rules

The system must deny on:

- checker timeout
- parser failure
- missing lease metadata where required
- expired lease
- lease validation error

The system must never silently fall back to allow.

## Observability Contract

Every agent must produce a local audit trail with records for:

- `BLOCK`
- `ALLOW_UNDER_LEASE`
- policy timeout or crash
- malformed input or missing metadata
- denied action after lease expiry

Minimum record fields:

- timestamp
- agent name
- enforcement surface
- decision
- summarized target
- reason code or explanation

Sensitive values must be redacted or omitted. Full secrets, raw credential values, and full request payload bodies must not be written to logs.

## Verification Strategy

Each agent must be tested against the same four behavioral cases:

1. benign action is allowed
2. risky action is blocked
3. risky action is allowed under an active valid lease
4. previously leased action is denied after expiry

Additional negative tests:

- malformed hook/tool payload
- checker timeout
- unavailable or unreadable lease state
- sensitive log content redaction

Verification inputs should cover:

- shell commands
- outbound network actions
- file write/edit actions
- sensitive-path targets

## Success Criteria

Phase 1 is complete when:

- all four Class 3 agents have active Safety controls through their real native/runtime surfaces
- high-risk shell, network, and file mutation actions are blocked by default
- temporary bypass leases work and expire cleanly
- each agent writes a usable local audit trail
- no agent is documented as having a hook surface it does not actually expose
- Codex and Antigravity are only counted as complete if a real mediation path exists for the surfaces being claimed; otherwise the phase remains open for those runtimes

## Risks and Constraints

- Gemini and Claude can likely achieve the closest match to the common policy model because they are hook-capable.
- Codex may require wrapper-based enforcement for anything beyond config and instruction hardening.
- Antigravity may require extension work rather than simple config edits.
- Because implementations are per-agent, drift risk is real and must be controlled later by the Observability and Control/Routing phases.

## Recommended Execution Boundary

The first implementation plan derived from this spec should:

- implement Gemini and Claude native safety first
- harden Codex through config plus real runtime choke points
- identify and patch the Antigravity execution broker rather than generic editor settings
- verify live behavior per agent before moving to Phase 1B context bootstrap
