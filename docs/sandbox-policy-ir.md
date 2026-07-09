# Sandbox Policy IR

**Status:** Implemented (pure policy evaluator)  
**Module:** `heiwa-protocol::sandbox_policy`  
**Enforcement:** OS backends TBD; fail-closed IR is the contract

## Tiers

| Tier | Name | Powers |
|------|------|--------|
| T0 | observe | Read allowlisted paths; no shell; network deny/local |
| T1 | workspace | R/W project dirs; limited network; shell in workspace |
| T2 | host_safe | Broader host read; limited network |
| T3 | elevated | Host shell/writes outside workspace → **require approval** |
| T4 | forbidden | Hard deny (credentials, other users, raw disk) |

## Verdicts

- `Allow`
- `Deny { reason }`
- `RequireApproval { reason }`

## Checks

- `check_read` / `check_write` / `check_network` / `check_shell` / `check_tool_risk`

Dangerous shell markers (`rm -rf /`, etc.) always **Deny**. Paths containing `/.ssh`, key material, SAM, etc. always **Deny**.

## Relation to existing types

Composes with `ExecutionScope`, `ToolLease`, `RiskClass`, and `NetworkPolicy` already in `heiwa-protocol`. New code should prefer `SandboxPolicy` for tiered agent sessions.
