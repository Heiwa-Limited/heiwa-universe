# Class 3 Runtime Safety Baseline

Task 1 baseline snapshot:

- Backup snapshot: `/Users/dmcgregsauce/tmp/class3-runtime-safety/20260330T171652Z/`
- Command run: `pytest scripts/tests/test_class3_runtime_safety.py -v`
- Result: `4 failed, 1 passed`

Failures from missing entrypoints:

- Gemini runtime policy hook: missing `/Users/dmcgregsauce/.gemini/hooks/runtime_policy.js`
- Claude pretool policy hook: missing `/Users/dmcgregsauce/.claude/plugins/devon-runtime-safety/hooks/pretool_policy.py`
- Codex safe wrapper: missing `/Users/dmcgregsauce/.codex/bin/codex-safe`

Passing baseline:

- Antigravity deny check passed for the explicit `DEVON_OPERATOR_ROOT` write attempt.

Baseline note:

- No incorrect-allow behavior was observed in the baseline run.
