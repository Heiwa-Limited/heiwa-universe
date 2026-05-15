# Operator Runbook

## Boot sequence

Read these before runtime changes:

1. `HEIWA.md`
2. `AGENTS.md`
3. `README.md`
4. `docs/current-capability.md`
5. `docs/architecture.md`
6. `docs/deployment.md`

## Basic checks

```bash
cargo test --offline -p heiwa-protocol -p heiwa_mcp -p heiwa-stdb -p heiwa-shell
cargo test --offline -p heiwa-core --test drex_provider_routing --test drex_scoring --test run_receipts --test worker_mesh
cargo test --offline -p heiwa-shell --test agentic_smoke
python3 scripts/validate_connector_manifests.py
bats tests/audit/test_connector_manifests.bats
bats tests/audit/test_audit_product_surface.bats
uv run --extra docs mkdocs build --strict
```

## Public surface rule

If a surface is not verified by tests or build checks, it should not be described as stack-complete in docs, README, or the static web shell.
