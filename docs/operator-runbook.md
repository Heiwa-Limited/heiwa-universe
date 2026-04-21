# Operator Runbook

## Boot sequence

Read these before runtime changes:

1. `HEIWA.md`
2. `AGENTS.md`
3. `BUILD_MATRIX.md`
4. `README.md`
5. `docs/deployment.md`

## Basic checks

```bash
bash scripts/check_runtime_baseline.sh
cargo build --workspace --locked
cargo test --workspace --locked
python -m pip install -r docs/requirements.txt
mkdocs build --strict
```

## Public surface rule

If a surface is not verified by tests or build checks, it should not be described as stack-complete in docs, README, or the static web shell.
