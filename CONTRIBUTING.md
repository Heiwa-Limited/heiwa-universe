# Contributing

Heiwa is being consolidated around a smaller public contract: installed runtime first, honest docs, reproducible builds, and explicit release surfaces.

Unless explicitly stated otherwise, contributions are submitted under the Apache License, Version 2.0. Do not include code, docs, generated assets, or dependency snapshots that cannot be distributed under that repository license.

## Start here

Read these first:

1. `HEIWA.md`
2. `AGENTS.md`
3. `BUILD_MATRIX.md`

## Development loop

Use the smallest loop that proves your change:

```bash
bash scripts/check_runtime_baseline.sh
cargo build --workspace --locked
cargo test --workspace --locked
uv run --extra dev python -m pytest
python -m venv .venv
source .venv/bin/activate
pip install -r docs/requirements.txt
mkdocs build --strict
```

If you change only a specific crate or docs page, run the targeted command first, then widen the check before merge.

The default Python gate covers maintained product/support tests. Legacy Hub tests under `apps/heiwa_hub/tests` are explicit repair targets and should be run by path when a change touches that legacy surface.

## Branch and worktree convention

- Claude worktrees live under `.worktrees/claude/<task-id>/`
- Codex worktrees live under `.worktrees/codex/<task-id>/`
- Branch names mirror the task id with the provider prefix
- Worktrees are short-lived and should be deleted after merge

If your environment cannot create nested git refs, use the closest safe fallback and note it in the handoff.

## Pull request expectations

- Keep changes scoped to one build-matrix task or one tightly related slice.
- Do not overstate maturity in docs or code comments.
- Preserve provider-owned behavior as provider-owned.
- Prefer local-first framing over hosted-control-plane framing unless the task is explicitly about legacy hosted surfaces.
- Include exact verification commands in the handoff.

## Docs and issues

- Keep docs aligned with `HEIWA.md` and `BUILD_MATRIX.md`.
- File bugs with a reproduction path and expected behavior.
- File platform requests as concrete build, release, docs, or packaging tasks.
