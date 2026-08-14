# Cockpit Lockfile Source-Truth Audit

Date: 2026-08-08
Plane: Evidence
Status: fixed on `codex/remove-orphan-cockpit-lock`

## Acquired Data

- Root `package.json` declares `apps/heiwa_app/clients/cockpit` as an npm
  workspace. That declaration was added after the nested lockfile so release
  preflight would install cockpit dependencies from the root graph.
- npm documents that lockfiles are honored only at the root project; workspace
  dependency resolution uses that single top-level lock. See
  [npm's package-lock documentation](https://docs.npmjs.com/cli/v11/configuring-npm/package-lock-json/).
- From the cockpit directory, `npm prefix` and `npm root` both resolve to the
  repository root.
- Root `package-lock.json` links `node_modules/@heiwa/cockpit` to the cockpit
  workspace and pins `solid-js 1.9.13` with patched `seroval 1.5.4`.
- The nested lockfile was stale: it pinned `solid-js 1.9.12`, vulnerable
  `seroval 1.5.2`, and `@solidjs/router ^0.15.0` even though cockpit
  `package.json` requires `@solidjs/router ^0.16.0`.
- A forced standalone dry-run rejected that nested graph because its router
  lock entry does not satisfy the current cockpit manifest. Standalone
  reproducibility was already broken, not preserved by retaining the file.
- No CI job, script, or active documentation referenced the nested lockfile.
- GitHub attributed seven open Dependabot alerts to the stale manifest,
  including critical `GHSA-mv8w-475r-vwqw`. The real root graph has no critical
  npm audit finding; its remaining non-critical findings are separate work.

## Root Cause

Cockpit started as a standalone package with its own lockfile, then became a
root npm workspace without removing the old lock. Two dependency graphs
survived: npm and CI used the root graph, while Dependabot also scanned the
orphaned graph. Updating from the cockpit directory therefore mutated the root
lock instead of the stale nested file.

## Decision

Delete `apps/heiwa_app/clients/cockpit/package-lock.json` and document the root
lock as the only cockpit dependency authority. Do not replace the stale file
with a regenerated nested lock while cockpit remains a root workspace.

The runtime baseline now resolves the root workspace patterns and rejects
`package-lock.json` inside any matching package. This does not affect
`apps/heiwa_app/desktop/package-lock.json` because desktop is an independent
package, not a declared root workspace.

If standalone cockpit installation becomes a real product requirement later,
make that an explicit packaging change: remove or redesign its workspace
membership, restore a deliberately managed lock, and add standalone CI. Do not
infer that contract from an orphaned file.

## Missing Data

- Dependabot alert closure is observable only after this change reaches the
  default branch and GitHub refreshes its dependency graph.
- The root npm graph still has non-critical audit findings. They are not caused
  by the deleted file and need their own narrow dependency update.

## Needed Data

- Post-merge confirmation that alerts attributed to the deleted manifest close.
- A separate root-lock audit fixing the remaining findings without broad,
  unrelated dependency churn.

## Verification Evidence

```bash
npm ci --ignore-scripts
npm ls seroval solid-js vite postcss @babel/core --all
npm --workspace @heiwa/cockpit run typecheck
npm --workspace @heiwa/cockpit run build
npm audit --omit=dev
bash scripts/check_agent_baseline.sh --branch codex/remove-orphan-cockpit-lock
```

Expected security truth after a clean root install: `seroval 1.5.4`, zero
critical npm audit findings, and no nested cockpit lockfile.

## Next Executable Action

After merge, query GitHub Dependabot alerts by `dependency.manifest_path` and
confirm that none still reference
`apps/heiwa_app/clients/cockpit/package-lock.json`. Then handle the real root
graph findings in a separate atomic branch.

## Equivalent-Capability Partner Handoff

Treat the root workspace declaration and root lock as one contract. Before
changing a nested package dependency, first run `npm prefix` and inspect the
root lock's workspace link. Report acquired, missing, and needed data; never
equate a scanner-visible manifest with a runtime-reachable graph without
proving which installer consumes it.
