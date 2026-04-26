# Product Surface

> Canonical map of tracked repo paths to surface classes. This file is read by `scripts/audit_product_surface.sh`. Update it when a path changes class; do not move class boundaries without checking `HEIWA.md` and `docs/audit/2026-04-25-slop-budget.md`.

**Last updated:** 2026-04-25
**Authority:** `HEIWA.md` defines what is product. This file labels tracked paths for repo hygiene and LOC accounting.

## Classes

| Class | Meaning |
| --- | --- |
| `product` | Active surfaces shipping in `heiwa`, companion runtime UX, repo release, or maintained sub-products |
| `generated` | Code or lockfiles emitted from a registered generator, package manager, or schema source |
| `legacy` | Old surfaces kept for migration/reference; not part of the public product contract |
| `reference` | Plans, design docs, audits, ADRs, continuity notes, or historical context |
| `archive` | Frozen snapshots or pointers to work no longer active in this repo |
| `vendored` | Third-party code copied into the repo |
| `runtime-artifact` | Logs, caches, spools, tmp data, local run output; should trend to zero tracked LOC |

## Path To Class

Longest prefix wins. Put narrower paths above broader parents when a child has a different class.

| Path | Class |
| --- | --- |
| `legacy/apps/heiwa_cli/runtime/logs` | runtime-artifact |
| `apps/heiwa_shell` | product |
| `apps/heiwa_core` | product |
| `apps/heiwa_app` | product |
| `apps/heiwa_orchestrator` | product |
| `apps/heiwa_trading` | product |
| `legacy/apps/heiwa_hub` | legacy |
| `legacy/apps/heiwa_cli` | legacy |
| `legacy/apps/heiwa_limbs` | legacy |
| `archive/apps/heiwa_dj` | archive |
| `apps/__init__.py` | legacy |
| `crates` | product |
| `packages/heiwa_bindings` | generated |
| `packages/heiwa_sdk` | product |
| `packages/heiwa_protocol` | product |
| `packages/heiwa_cli` | product |
| `packages/heiwa_identity` | product |
| `legacy/packages/heiwa_skills` | legacy |
| `packages/heiwa_cognition` | legacy |
| `packages/heiwa_ui` | legacy |
| `packages/heiwa_knowledge` | legacy |
| `packages/__init__.py` | product |
| `runtime/python` | product |
| `runtime/fleets` | runtime-artifact |
| `runtime/spool` | runtime-artifact |
| `runtime/logs` | runtime-artifact |
| `docs/superpowers` | reference |
| `docs/design` | reference |
| `docs/audit` | reference |
| `docs/enterprise` | reference |
| `docs/standards` | product |
| `docs` | product |
| `ops/research` | reference |
| `ops/docs_and_deps` | vendored |
| `ops` | product |
| `scripts` | product |
| `tests` | product |
| `infra` | product |
| `config` | product |
| `bin` | product |
| `node` | legacy |
| `policies` | product |
| `memory` | reference |
| `plans` | reference |
| `.claude/agents` | generated |
| `.claude` | product |
| `.codex` | product |
| `.gemini/agents` | generated |
| `.gemini` | product |
| `.github` | product |
| `.ollama` | product |
| `.openclaw` | legacy |
| `.wrangler` | runtime-artifact |
| `Cargo.lock` | generated |
| `Cargo.toml` | product |
| `package-lock.json` | generated |
| `package.json` | product |
| `uv.lock` | generated |
| `pyproject.toml` | product |
| `requirements.txt` | product |
| `README.md` | product |
| `LICENSE` | product |
| `HEIWA.md` | product |
| `AGENTS.md` | product |
| `CLAUDE.md` | product |
| `GEMINI.md` | product |
| `IDENTITY.md` | reference |
| `SOUL.md` | reference |
| `SECURITY.md` | product |
| `CONTRIBUTING.md` | product |
| `CONTRIBUTORS.md` | product |
| `CODE_OF_CONDUCT.md` | product |
| `BUILD_MATRIX.md` | reference |
| `PRODUCT_SURFACE.md` | product |
| `mkdocs.yml` | product |
| `biome.json` | product |
| `tsconfig.base.json` | product |
| `rust-toolchain.toml` | product |
| `conftest.py` | product |
| `justfile` | product |
| `.dockerignore` | product |
| `.env.example` | product |
| `.geminiignore` | product |
| `.gitignore` | product |
| `.mcp.json` | product |
| `.node-version` | product |
| `.nvmrc` | product |
| `.pyre_configuration` | product |

## Notes

- `apps/heiwa_trading` is an active sub-product, not slop.
- `legacy/packages/heiwa_skills` is the largest legacy surface, quarantined under `legacy/` per the slop quarantine plan.
- `legacy/apps/heiwa_hub` is the legacy Python ops surface. It remains present but is not the current public operator path.
- `runtime/python` is source and remains product for now. Runtime spools, logs, and fleet start artifacts are `runtime-artifact`.
- `docs/audit` is reference but contains operational baselines. Do not delete entries without replacing their evidence.
- Generated bindings and lockfiles are not slop by default, but their LOC should stay reproducible from a source schema or package manifest.

## Audit Rule

The audit script walks `git ls-files`, finds the longest-prefix match in this table, sums LOC per class, and reports any unmatched paths as `unclassified`. The target for `unclassified` is zero.
