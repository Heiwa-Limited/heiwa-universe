# Status: Class 3 Agent Runtime Safety (Phase 1A Outcome)

**Date:** 2026-03-30
**Stage:** `Safety`
**Outcome:** ✅ COMPLETE

## Summary

Phase 1A of the Class 3 runtime safety rollout is complete. All four executive agents—Gemini, Claude, Codex, and Antigravity—now have active safety controls through their native or real execution control planes.

## Enforcement Matrix

| Agent | Surface | Posture | Audit Trail |
| :--- | :--- | :--- | :--- |
| **Gemini CLI** | `BeforeTool` Hook | Fail-closed, Regex-based | `~/.gemini/runtime-safety/audit.jsonl` |
| **Claude Code** | `PreToolUse` Hook | Fail-closed, Allowlist-based | `~/.claude/runtime-safety/pretool-YYYY-MM-DD.jsonl` |
| **Codex** | Config + Wrapper | Launch-hardened | Local session logs |
| **Antigravity** | Broker Policy | Explicit Deny-by-default | Broker dispatch logs |

## Key Achievements

1.  **Fail-Closed Baseline**: Gemini and Claude hooks verified to deny on malformed JSON, checker timeout, or internal policy errors.
2.  **Surface Coverage**:
    *   **Shell**: Destructive `rm`, `mkfs`, `dd` patterns blocked.
    *   **Network**: Outbound mutation (POST/PUT/PATCH) and ambiguous `WebFetch` calls denied by default.
    *   **File**: Sensitive paths (`~/.ssh`, `~/.gemini`, etc.) protected against unauthorized writes/edits.
3.  **Honest Posture**: Codex and Antigravity are hardened through their actual runtime surfaces (wrapper/broker) rather than simulated hook parity.
4.  **Auditability**: Every agent produces a local audit trail documenting block decisions.

## Known Limits (Phase 1A)

- **Lease Identity**: Emergency bypass leases are currently **DORMANT** for Gemini and Claude because hook payloads do not yet expose stable `proposal_id` or `session_id` fields required for high-fidelity STDB lookup.
- **Drift**: Enforcement logic is native per agent; cross-agent behavioral parity will be tightened in the `Control/routing` phase.

## Next Steps

- **Phase 1B**: `Context bootstrap` — using `SessionStart` and `BeforeAgent` hooks to inject project state.
- **Phase 2**: `Observability` — centralizing audit logs into the SpacetimeDB ledger.
