# Class 3 Agent Runtime Safety — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Roll out Phase 1 `Safety` across Gemini, Claude, Codex, and Antigravity using each runtime's real control plane, with fail-closed defaults, hybrid bypass leases, shell/network/file-write coverage where mediation is real, and local audit trails.

**Architecture:** Gemini and Claude enforce safety natively through hook entrypoints. Their policy evaluators should use the existing Heiwa SpacetimeDB lease query surface where hook payloads expose enough identity to map an action to a `capability_leases` row; otherwise they must fail closed and keep the lease-allow path disabled until identity is available. Codex is hardened honestly at launch/config/instruction level through a safe wrapper and stricter defaults, not fake hook parity. Antigravity is wired explicitly to the archived `devonx` broker through user settings, and safety lives in the broker's dispatch/approval path rather than in the extension shell.

**Tech Stack:** Node.js, Python 3.14, Gemini CLI hooks, Claude Code hooks, Codex CLI, Antigravity Devon Operator extension, Heiwa SDK SpacetimeDB bridge, pytest, JSON, TOML

**Spec:** `docs/superpowers/specs/2026-03-30-class3-agent-runtime-safety-design.md`

---

## Constraints

- No native Codex hook surface was found in the current CLI or `~/.codex/config.toml`. Do not claim tool-level parity without a new broker.
- Antigravity's real mediation surface is `devonx`, not the empty `~/.antigravity/extensions/devon-operator-v1/` directory.
- `~/.dmcgregsauce/operator` is a broken symlink. Prefer explicit Antigravity settings over implicit fallbacks.
- `packages/heiwa_sdk/heiwa_sdk/hooks.py` and `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` already expose the authoritative `capability_leases` query path. Phase 1 should consume that API, not redesign it.
- Home-directory config edits are live system state, not a clean repo-local feature branch. Back up every target file before mutation.

## File Map

### New files (authored)

| File | Responsibility |
|------|---------------|
| `scripts/tests/test_class3_runtime_safety.py` | Repo-tracked verification harness for Gemini, Claude, Codex, and Antigravity safety smoke tests |
| `/Users/dmcgregsauce/.gemini/hooks/runtime_policy.js` | Gemini policy evaluator for shell, network, and file-mutation tools |
| `/Users/dmcgregsauce/.gemini/hooks/query_capability_lease.py` | Gemini helper that calls Heiwa's existing SpacetimeDB lease lookup |
| `/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/README.md` | Local plugin-style packaging notes for Claude safety hooks |
| `/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/hooks.json` | Source-of-truth Claude `PreToolUse` matcher layout |
| `/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/pretool_policy.py` | Claude `PreToolUse` policy evaluator |
| `/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/query_capability_lease.py` | Claude helper that calls Heiwa's existing SpacetimeDB lease lookup |
| `/Users/dmcgregsauce/.codex/bin/codex-safe` | Safe Codex launcher that blocks unsafe CLI flags, writes audits, and applies safer defaults |

### Modified files

| File | Change |
|------|--------|
| `/Users/dmcgregsauce/.gemini/settings.json` | Expand `BeforeTool` matchers from shell-only to the actual high-risk Gemini tools |
| `/Users/dmcgregsauce/.gemini/hooks/dangerous_check.js` | Reduce to a compatibility shim or delegate to `runtime_policy.js` |
| `/Users/dmcgregsauce/.claude/settings.local.json` | Register `PreToolUse` hooks and narrow machine-local safety posture without breaking existing plugin installs |
| `/Users/dmcgregsauce/.codex/config.toml` | Harden launch defaults for future Codex sessions |
| `/Users/dmcgregsauce/.codex/AGENTS.md` | Document the safe-launch requirement and the honest limits of Codex Phase 1 mediation |
| `/Users/dmcgregsauce/Library/Application Support/Antigravity/User/settings.json` | Point Antigravity explicitly at the archived operator root and `devonx` binary |
| `/Users/dmcgregsauce/heiwa_archive/heiwa-core/bin/heiwa_devonx_legacy.py` | Add Phase 1 safety classification, lease checks where possible, and audit logging in the actual broker path |
| `/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator/config/policies/read_only_default.json` | Tighten the documented operator safety posture only if `devonx doctor` confirms this file is consumed |

### Reference files (read-only)

| File | Why it matters |
|------|----------------|
| `packages/heiwa_sdk/heiwa_sdk/hooks.py` | Existing execution-hook semantics and audit behavior |
| `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` | Authoritative `get_active_capability_lease()` lookup |
| `/Users/dmcgregsauce/.antigravity/extensions/heiwa.devon-operator-v1-1.0.0/src/extension.ts` | Confirms Antigravity resolves `operatorRoot`, `devonxPath`, and state paths from settings |
| `/Users/dmcgregsauce/.claude/plugins/cache/claude-plugins-official/security-guidance/15268f03d2f5/hooks/hooks.json` | Known-good Claude plugin hook packaging example |
| `/Users/dmcgregsauce/.gemini/extensions/superpowers/skills/using-superpowers/references/gemini-tools.md` | Gemini tool-name map for `run_shell_command`, `write_file`, `replace`, `web_fetch`, and `google_web_search` |

---

### Task 1: Capture baselines and write the failing verification harness

**Files:**
- Create: `scripts/tests/test_class3_runtime_safety.py`
- Backup target files into: `/Users/dmcgregsauce/tmp/class3-runtime-safety/`

- [ ] **Step 1: Create a timestamped backup directory and copy the current live configs**

Run:

```bash
ts="$(date -u +%Y%m%dT%H%M%SZ)"
backup_root="/Users/dmcgregsauce/tmp/class3-runtime-safety/${ts}"
mkdir -p "${backup_root}"
cp /Users/dmcgregsauce/.gemini/settings.json "${backup_root}/gemini.settings.json"
cp /Users/dmcgregsauce/.gemini/hooks/dangerous_check.js "${backup_root}/gemini.dangerous_check.js"
cp /Users/dmcgregsauce/.claude/settings.json "${backup_root}/claude.settings.json"
cp /Users/dmcgregsauce/.claude/settings.local.json "${backup_root}/claude.settings.local.json"
cp /Users/dmcgregsauce/.codex/config.toml "${backup_root}/codex.config.toml"
cp /Users/dmcgregsauce/.codex/AGENTS.md "${backup_root}/codex.AGENTS.md"
cp "/Users/dmcgregsauce/Library/Application Support/Antigravity/User/settings.json" "${backup_root}/antigravity.settings.json"
cp /Users/dmcgregsauce/heiwa_archive/heiwa-core/bin/heiwa_devonx_legacy.py "${backup_root}/heiwa_devonx_legacy.py"
```

Expected: all target files are preserved under one timestamped rollback directory before any live mutation.

- [ ] **Step 2: Create the cross-agent smoke test harness**

`scripts/tests/test_class3_runtime_safety.py`:

```python
from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

HOME = Path("/Users/dmcgregsauce")
GEMINI_POLICY = HOME / ".gemini/hooks/runtime_policy.js"
CLAUDE_POLICY = HOME / ".claude/plugins/devon-runtime-safety/hooks/pretool_policy.py"
CODEX_SAFE = HOME / ".codex/bin/codex-safe"
DEVONX = Path("/Users/dmcgregsauce/heiwa_archive/heiwa-core/bin/devonx")


def run_json_command(cmd: list[str], payload: dict) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        input=json.dumps(payload),
        text=True,
        capture_output=True,
        check=False,
    )


def test_gemini_blocks_root_delete():
    payload = {"tool_name": "run_shell_command", "tool_input": {"command": "rm -rf /"}}
    proc = run_json_command(["node", str(GEMINI_POLICY)], payload)
    result = json.loads(proc.stdout)
    assert result["decision"] == "deny"


def test_gemini_blocks_sensitive_write():
    payload = {"tool_name": "write_file", "tool_input": {"path": "/Users/dmcgregsauce/.ssh/config"}}
    proc = run_json_command(["node", str(GEMINI_POLICY)], payload)
    result = json.loads(proc.stdout)
    assert result["decision"] == "deny"


def test_claude_blocks_network_post():
    payload = {"tool_name": "WebFetch", "tool_input": {"url": "https://example.com", "method": "POST"}}
    proc = run_json_command(["python3", str(CLAUDE_POLICY)], payload)
    result = json.loads(proc.stdout)
    assert result["decision"] == "deny"


def test_codex_safe_rejects_dangerous_bypass_flag():
    proc = subprocess.run(
        [str(CODEX_SAFE), "--dangerously-bypass-approvals-and-sandbox", "--help"],
        text=True,
        capture_output=True,
        check=False,
    )
    assert proc.returncode != 0
    assert "blocked" in proc.stderr.lower()


def test_antigravity_operator_denies_off_limits_write():
    proc = subprocess.run(
        [
            str(DEVONX),
            "dispatch",
            "submit",
            "--json",
            "--from",
            "antigravity",
            "--action",
            "write-file",
            "--target-surface",
            "filesystem",
            "--target-scope",
            "/Users/dmcgregsauce/.gemini/settings.json",
            "--mode",
            "write",
        ],
        text=True,
        capture_output=True,
        check=False,
        env={
            **os.environ,
            "DEVON_OPERATOR_ROOT": "/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator",
        },
    )
    assert proc.returncode == 0
    assert '"status": "denied"' in proc.stdout or '"status":"denied"' in proc.stdout
```

- [ ] **Step 3: Run the harness before implementation and confirm it fails for the right reasons**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest scripts/tests/test_class3_runtime_safety.py -v
```

Expected: FAIL because the new policy scripts and wrapper do not exist yet, or because current behavior still allows actions that should be denied.

- [ ] **Step 4: Record the baseline failures in the task notes**

Expected: a short handoff note listing which tests failed because of missing files versus incorrect allow behavior. This becomes the before-state for the rollout.

---

### Task 2: Expand Gemini from shell regex blocking to a real tool-family policy evaluator

**Files:**
- Create: `/Users/dmcgregsauce/.gemini/hooks/runtime_policy.js`
- Create: `/Users/dmcgregsauce/.gemini/hooks/query_capability_lease.py`
- Modify: `/Users/dmcgregsauce/.gemini/settings.json`
- Modify: `/Users/dmcgregsauce/.gemini/hooks/dangerous_check.js`

- [ ] **Step 1: Capture one full Gemini `BeforeTool` payload before relying on lease metadata**

Run a temporary debug hook or add short-lived stderr logging so the full JSON payload for `run_shell_command` and `write_file` is captured into the backup directory.

Expected: confirmation of whether Gemini exposes stable identity fields that can map to `proposal_id` and `holder_id`. If those fields are absent, the allow-under-lease path stays disabled and all high-risk matches remain fail-closed.

- [ ] **Step 2: Write the Gemini lease lookup helper**

`/Users/dmcgregsauce/.gemini/hooks/query_capability_lease.py`:

```python
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

sys.path.insert(0, "/Users/dmcgregsauce/heiwa/packages/heiwa_sdk")

from heiwa_sdk.spacetimedb import SpacetimeDB


def main() -> int:
    payload = json.loads(sys.stdin.read() or "{}")
    proposal_id = payload.get("proposal_id")
    holder_id = payload.get("holder_id")
    if not proposal_id or not holder_id:
        print(json.dumps({"ok": False, "reason": "missing_identity"}))
        return 0
    db = SpacetimeDB(
        db_identity=os.environ.get("STDB_IDENTITY", "heiwaproductiondb"),
        server=os.environ.get("STDB_SERVER", "local"),
    )
    lease = db.get_active_capability_lease(proposal_id, holder_id)
    print(json.dumps({"ok": bool(lease), "lease": lease or None}))
    return 0
```

- [ ] **Step 3: Implement the Gemini policy evaluator**

`/Users/dmcgregsauce/.gemini/hooks/runtime_policy.js`:

```javascript
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const AUDIT_LOG = "/Users/dmcgregsauce/.gemini/runtime-safety/audit.jsonl";
const SENSITIVE_PREFIXES = [
  "/Users/dmcgregsauce/.ssh",
  "/Users/dmcgregsauce/.gemini",
  "/Users/dmcgregsauce/.claude",
  "/Users/dmcgregsauce/.codex",
  "/Users/dmcgregsauce/Library/Application Support/Antigravity/User",
];

function classify(toolName, input) {
  if (toolName === "run_shell_command") return { surface: "shell", target: input.command || "" };
  if (toolName === "write_file" || toolName === "replace") return { surface: "file", target: input.path || "" };
  if (toolName === "web_fetch" || toolName === "google_web_search") return { surface: "network", target: input.url || input.query || "" };
  return { surface: "other", target: "" };
}
```

Required behavior:

- parse stdin and deny on parser failure
- classify `run_shell_command`, `write_file`, `replace`, `web_fetch`, and `google_web_search`
- block destructive shell patterns, credential exfiltration patterns, outbound mutation methods, and writes into sensitive prefixes
- call `query_capability_lease.py` only when the action would otherwise be denied and identity metadata is available
- append one JSONL audit record per `allow`, `deny`, timeout, or policy error without logging secret values
- return Gemini-compatible hook JSON with `decision`, `reason`, and optional `systemMessage`

- [ ] **Step 4: Preserve the existing hook command path**

Replace `/Users/dmcgregsauce/.gemini/hooks/dangerous_check.js` with a compatibility shim:

```javascript
require("./runtime_policy");
```

Expected: existing hook references do not break during rollout, even if `settings.json` is updated in the same change.

- [ ] **Step 5: Expand Gemini `BeforeTool` matching to the full Phase 1 surface**

Update `/Users/dmcgregsauce/.gemini/settings.json` so the `BeforeTool` matcher covers:

```json
"matcher": "run_shell_command|write_file|replace|web_fetch|google_web_search"
```

Expected: Gemini evaluates the real high-risk tool family instead of only shell commands.

- [ ] **Step 6: Re-run Gemini tests and smoke commands**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest scripts/tests/test_class3_runtime_safety.py -k "gemini" -v
printf '%s' '{"tool_name":"run_shell_command","tool_input":{"command":"rm -rf /"}}' | node /Users/dmcgregsauce/.gemini/hooks/runtime_policy.js
printf '%s' '{"tool_name":"write_file","tool_input":{"path":"/Users/dmcgregsauce/.ssh/config"}}' | node /Users/dmcgregsauce/.gemini/hooks/runtime_policy.js
printf '%s' '{"tool_name":"web_fetch","tool_input":{"url":"https://example.com","method":"POST"}}' | node /Users/dmcgregsauce/.gemini/hooks/runtime_policy.js
```

Expected: all three manual samples return `decision: "deny"` and the pytest Gemini cases pass.

---

### Task 3: Package Claude safety as a local plugin-style `PreToolUse` implementation

**Files:**
- Create: `/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/README.md`
- Create: `/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/hooks.json`
- Create: `/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/pretool_policy.py`
- Create: `/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/query_capability_lease.py`
- Modify: `/Users/dmcgregsauce/.claude/settings.local.json`

- [ ] **Step 1: Capture real Claude hook payloads for `Bash`, `Write`, and `WebFetch`**

Register a short-lived echo hook that writes stdin to the backup directory, then trigger one benign `Bash`, one `Write`, and one `WebFetch`.

Expected: confirmed payload schema and tool names. If the payload omits stable identity fields, keep the lease-allow path disabled and rely on hard blocks plus audits.

- [ ] **Step 2: Create the local plugin-style directory and README**

`/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/README.md`:

```markdown
# Devon Runtime Safety

Local Claude safety package for `PreToolUse` enforcement.

- Packaging is plugin-style for maintainability.
- Hook registration stays in `~/.claude/settings.local.json`.
- Lease lookup uses Heiwa's existing SpacetimeDB bridge when the hook payload exposes enough identity.
```

- [ ] **Step 3: Add Claude hook source-of-truth matchers**

`/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/hooks.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [{ "type": "command", "command": "python3 /Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/pretool_policy.py" }]
      },
      {
        "matcher": "Write|Edit|MultiEdit",
        "hooks": [{ "type": "command", "command": "python3 /Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/pretool_policy.py" }]
      },
      {
        "matcher": "WebFetch|WebSearch",
        "hooks": [{ "type": "command", "command": "python3 /Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/pretool_policy.py" }]
      }
    ]
  }
}
```

- [ ] **Step 4: Implement the Claude `PreToolUse` policy evaluator**

`/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/pretool_policy.py`:

```python
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

AUDIT_LOG = Path("/Users/dmcgregsauce/.claude/runtime-safety/audit.jsonl")
SENSITIVE_PREFIXES = (
    "/Users/dmcgregsauce/.ssh",
    "/Users/dmcgregsauce/.claude",
    "/Users/dmcgregsauce/.codex",
    "/Users/dmcgregsauce/.gemini",
)
```

Required behavior:

- deny on malformed JSON or missing required fields
- classify `Bash`, `Write`, `Edit`, `MultiEdit`, `WebFetch`, and `WebSearch`
- block sensitive writes, destructive shell patterns, and outbound mutation methods by default
- query `query_capability_lease.py` only when the hook payload exposes enough identity and the request would otherwise be denied
- write JSONL audits under `~/.claude/runtime-safety/`
- return Claude-compatible `allow` or `deny` responses with a human-readable reason

- [ ] **Step 5: Register the hooks in `settings.local.json`**

Add a local `hooks` block to `/Users/dmcgregsauce/.claude/settings.local.json` that mirrors `hooks.json`, keeping the change machine-local and not dependent on undocumented plugin install mechanics.

Expected: Claude loads the new `PreToolUse` policy directly from local settings while the hook source stays packaged under `~/.claude/plugins/devon-runtime-safety/`.

- [ ] **Step 6: Run the Claude verification loop**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest scripts/tests/test_class3_runtime_safety.py -k "claude" -v
printf '%s' '{"tool_name":"Write","tool_input":{"file_path":"/Users/dmcgregsauce/.ssh/config","content":"x"}}' | python3 /Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/pretool_policy.py
printf '%s' '{"tool_name":"WebFetch","tool_input":{"url":"https://example.com","method":"POST"}}' | python3 /Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/pretool_policy.py
```

Expected: Claude smoke tests return `deny` and the Claude pytest cases pass.

---

### Task 4: Harden Codex honestly through launch posture, config, and operator guidance

**Files:**
- Create: `/Users/dmcgregsauce/.codex/bin/codex-safe`
- Modify: `/Users/dmcgregsauce/.codex/config.toml`
- Modify: `/Users/dmcgregsauce/.codex/AGENTS.md`

- [ ] **Step 1: Write the failing Codex wrapper tests first**

Expand `scripts/tests/test_class3_runtime_safety.py` with:

```python
def test_codex_safe_applies_workspace_write_defaults():
    proc = subprocess.run([str(CODEX_SAFE), "--help"], text=True, capture_output=True, check=False)
    assert proc.returncode == 0


def test_codex_safe_rejects_search_without_explicit_override():
    proc = subprocess.run([str(CODEX_SAFE), "--search", "--help"], text=True, capture_output=True, check=False)
    assert proc.returncode != 0
```

- [ ] **Step 2: Implement the safe launcher**

`/Users/dmcgregsauce/.codex/bin/codex-safe`:

```bash
#!/usr/bin/env bash
set -euo pipefail

blocked_flags=(
  "--dangerously-bypass-approvals-and-sandbox"
  "--search"
)

prev=""
for arg in "$@"; do
  for blocked in "${blocked_flags[@]}"; do
    if [[ "$arg" == "$blocked" ]]; then
      printf 'blocked by codex-safe: %s\n' "$arg" >&2
      exit 64
    fi
  done
  if [[ "$prev" == "-a" || "$prev" == "--ask-for-approval" ]]; then
    if [[ "$arg" == "never" ]]; then
      printf 'blocked by codex-safe: -a/--ask-for-approval never\n' >&2
      exit 64
    fi
  fi
  if [[ "$prev" == "-s" || "$prev" == "--sandbox" ]]; then
    if [[ "$arg" == "danger-full-access" ]]; then
      printf 'blocked by codex-safe: -s/--sandbox danger-full-access\n' >&2
      exit 64
    fi
  fi
  prev="$arg"
done

exec codex -a untrusted -s workspace-write "$@"
```

Required behavior:

- reject `--dangerously-bypass-approvals-and-sandbox`
- reject `--search` unless a future explicit lease-aware override path exists
- reject `-a never` and `-s danger-full-access` if passed manually
- append one JSONL audit record under `~/.codex/runtime-safety/audit.jsonl`
- never claim to mediate in-session write or network tools after launch

- [ ] **Step 3: Harden future Codex sessions in `config.toml`**

Update `/Users/dmcgregsauce/.codex/config.toml` defaults to:

```toml
approval_policy = "untrusted"
sandbox_mode = "workspace-write"
```

Leave a comment or adjacent note in the handoff that the current running session is unaffected until restarted.

- [ ] **Step 4: Document the honest limits in `~/.codex/AGENTS.md`**

Add a short Phase 1 section stating:

- `codex-safe` is the supported launcher for autonomous runs
- native hook parity does not exist in the current Codex CLI
- launch-time safety is enforced; in-session network/file mediation is not guaranteed in Phase 1

- [ ] **Step 5: Prove the launch-time enforcement works and stop there**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest scripts/tests/test_class3_runtime_safety.py -k "codex" -v
/Users/dmcgregsauce/.codex/bin/codex-safe --dangerously-bypass-approvals-and-sandbox --help
/Users/dmcgregsauce/.codex/bin/codex-safe --search --help
/Users/dmcgregsauce/.codex/bin/codex-safe --help
```

Expected:

- the first two commands exit non-zero with a `blocked by codex-safe` message
- the plain `--help` invocation succeeds
- the Codex tests pass
- the task notes explicitly record that Codex Phase 1 is launch-hardened, not hook-parity complete

---

### Task 5: Rebind Antigravity to the real `devonx` broker and harden dispatch safety there

**Files:**
- Modify: `/Users/dmcgregsauce/Library/Application Support/Antigravity/User/settings.json`
- Modify: `/Users/dmcgregsauce/heiwa_archive/heiwa-core/bin/heiwa_devonx_legacy.py`
- Modify if consumed: `/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator/config/policies/read_only_default.json`

- [ ] **Step 1: Point Antigravity explicitly at the archived operator root and CLI**

Update `/Users/dmcgregsauce/Library/Application Support/Antigravity/User/settings.json` with:

```json
{
  "devonOperator.operatorRoot": "/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator",
  "devonOperator.devonxPath": "/Users/dmcgregsauce/heiwa_archive/heiwa-core/bin/devonx",
  "devonOperator.statusPath": "/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator/state/ide/antigravity/status.json",
  "devonOperator.inboxPath": "/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator/state/ide/antigravity/inbox",
  "devonOperator.outboxPath": "/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator/state/ide/antigravity/outbox"
}
```

Expected: Antigravity no longer depends on the broken `~/.dmcgregsauce/operator` symlink for broker discovery.

- [ ] **Step 2: Confirm the broker resolves before editing policy logic**

Run:

```bash
DEVON_OPERATOR_ROOT=/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator /Users/dmcgregsauce/heiwa_archive/heiwa-core/bin/devonx doctor
DEVON_OPERATOR_ROOT=/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator /Users/dmcgregsauce/heiwa_archive/heiwa-core/bin/devonx adapters status
```

Expected: both commands succeed and report the archived operator root rather than a missing path.

- [ ] **Step 3: Add explicit Phase 1 safety classification inside `heiwa_devonx_legacy.py`**

Implement focused helpers in `/Users/dmcgregsauce/heiwa_archive/heiwa-core/bin/heiwa_devonx_legacy.py`:

```python
def classify_dispatch_request(request: dict[str, Any]) -> dict[str, Any]:
    ...

def lookup_capability_lease(request: dict[str, Any]) -> dict[str, Any] | None:
    ...

def is_sensitive_target(target_scope: str) -> bool:
    ...

def append_runtime_safety_audit(event: dict[str, Any]) -> None:
    ...
```

Required behavior:

- deny destructive filesystem writes to sensitive roots by default
- deny outbound mutation-like actions by default
- keep existing approval-gated semantics where they already exist, but add fail-closed classification and audit records before execution
- use STDB-backed lease lookup only when the dispatch request carries enough identity to map to `capability_leases`; otherwise keep lease-based allow paths disabled and preserve hard-deny behavior
- do not patch the Antigravity extension unless broker execution proves impossible

- [ ] **Step 4: Tighten the documented operator policy only if the runtime consumes it**

Run `devonx doctor` or inspect the policy loader path. If `/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator/config/policies/read_only_default.json` is actually read at runtime, update it so the documented policy matches the new broker behavior.

Expected: no dead config churn. Only edit the JSON if the broker truly consumes it.

- [ ] **Step 5: Prove Antigravity denial paths through `devonx`**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest scripts/tests/test_class3_runtime_safety.py -k "antigravity" -v
DEVON_OPERATOR_ROOT=/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator /Users/dmcgregsauce/heiwa_archive/heiwa-core/bin/devonx dispatch submit --json --from antigravity --action write-file --target-surface filesystem --target-scope /Users/dmcgregsauce/.gemini/settings.json --mode write
DEVON_OPERATOR_ROOT=/Users/dmcgregsauce/heiwa_archive/heiwa-core/legacy/devonx_operator /Users/dmcgregsauce/heiwa_archive/heiwa-core/bin/devonx dispatch submit --json --from antigravity --action post-request --target-surface network --target-scope https://example.com --mode write
```

Expected: both dispatch submissions return a denied or approval-required result, and the Antigravity pytest cases pass.

---

### Task 6: Run the full verification matrix and publish the honest Phase 1 outcome

**Files:**
- Modify: `scripts/tests/test_class3_runtime_safety.py`
- Update task notes / handoff summary with results

- [ ] **Step 1: Add the final lease and malformed-input cases to the harness**

Extend `scripts/tests/test_class3_runtime_safety.py` with:

- malformed JSON / parser-failure denial checks for Gemini and Claude
- one lease-covered allow case for Gemini or Claude if stable identity fields were discovered
- one expiry check proving the lease path falls back to deny after TTL
- one Codex assertion that launch-time hardening is present but no hook-level mediation claim is made

- [ ] **Step 2: Run the full matrix**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest scripts/tests/test_class3_runtime_safety.py -v
```

Expected: all implemented cases pass. If a lease-allow case cannot be wired because the runtime does not expose stable identity, that test should be marked `xfail` with a reason instead of silently skipped.

- [ ] **Step 3: Capture a four-agent handoff summary**

Record, in one short note:

- Gemini: hook-complete or hook-complete-with-lease-disabled
- Claude: hook-complete or hook-complete-with-lease-disabled
- Codex: launch-hardened only
- Antigravity: broker-hardened or broker-hardened-with-lease-disabled

Expected: the final handoff says exactly what was enforced and what remains open. No parity claims that the runtime cannot support.

- [ ] **Step 4: Only then move to Stage 1B (`Context bootstrap`)**

Expected: Phase 1A ends with a verified safety baseline and an honest residual-risk note, not with speculative bootstrap work bundled into the same rollout.
