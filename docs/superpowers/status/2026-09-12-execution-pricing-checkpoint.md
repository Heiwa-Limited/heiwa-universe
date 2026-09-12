# Execution, pricing, and publishing checkpoint

Recorded: 2026-09-12. Plane: Execution / Evidence.

## Source promotion

The integrated checkpoint landed through [PR #97](https://github.com/Heiwa-Limited/heiwa-universe/pull/97)
and reached production source through [PR #92](https://github.com/Heiwa-Limited/heiwa-universe/pull/92).
It includes the worker repairs from #95, the DREX price-truth work from #96
with additional review fixes, and the completed publishing corrections from #93.

| Boundary | Revision |
| --- | --- |
| Reviewed checkpoint candidate | `1134be953e6671af4bbbb1109f1d297f13063fab` |
| Verified integration on `dev` | `073eed1bb0859ba9612d53d65fcc99899d0dba8f` |
| Production source on `main` | `e0e68db5b540a25d40ccbca1ed0793934dbcbf1c` |

The integration and production revisions have identical source trees. Both
merges followed protected PR checks with the expected head pinned at merge.
All three worker review threads and the publishing topology finding were
resolved against the implemented fixes. Greptile's latest checkpoint comment
reported an expired trial; it provided no new code review. Codex inspected the
combined diff and reproduced the additional routing failures before fixing them.

## Verified behavior

1. Worker execution uses the canonical executable path that was hashed for
   its receipt. A failed initial heartbeat kills and reaps the child. A
   sensitivity-screen refusal of pane output still permits worker exit evidence.
2. DREX distinguishes known prices from unknown rates within and across
   accounts. Unknown prices cannot satisfy a cost ceiling. Without a ceiling,
   they are a last resort and choose the smallest sufficient model.
3. Models saved without price evidence load as unknown. Discovery or explicit
   configuration must establish rates before those models can satisfy a budget;
   explicitly known free models retain zero-budget eligibility after persistence.
4. Public shell and docs ownership are independent, workflow Node setup follows
   the repository pin, and Pages builds locked strict docs from protected `main`.
5. Desktop Vitest is patched to 4.1.11 for
   [GHSA-82fw-gwwq-j7x9](https://github.com/advisories/GHSA-82fw-gwwq-j7x9).
   Greptile's generated AI-fix links are disabled because their payloads directed
   agents to push to protected branches. Historical comments can retain old links.

## Acceptance evidence

- The required local gate passed all 31 executed checks on clean, unchanged
  candidate and integration revisions. Private receipts are
  `local-ci-0sw_lhip/receipt.json` and `local-ci-wlhnkdqa/receipt.json`.
- [Candidate CI](https://github.com/Heiwa-Limited/heiwa-universe/actions/runs/34681358817)
  and [promotion PR CI](https://github.com/Heiwa-Limited/heiwa-universe/actions/runs/34681749472)
  each passed all 11 checks, including native desktop tests, Clippy, and release
  compilation. Exact heads and unresolved review threads were rechecked before merge.
- Three additional DREX cases failed before the corrections: cross-account
  price ordering, unknown fallback model size, and legacy cloud entries passing
  a zero-dollar ceiling. All 23 DREX tests then passed. Provider tests also passed;
  credential-dependent live tests retain their explicit opt-in requirements.
- Desktop verification passed 83 Vitest cases, seven build-helper cases,
  typechecking, and the production web build. Both root and desktop npm audits
  reported zero vulnerabilities. Local L0, L1, and L2 acceptance passed.
- Public topology regression tests passed. The GitHub Pages configuration was
  rechecked: `docs.heiwa.ltd`, workflow publishing, and a `main`-only environment.

## Release and product limits

This receipt establishes the source promotion and its preceding acceptance
evidence. Binary publication separately requires successful
[main CI](https://github.com/Heiwa-Limited/heiwa-universe/actions/runs/34681916098)
and [main certification](https://github.com/Heiwa-Limited/heiwa-universe/actions/runs/34681916083)
at the production revision, followed by release assets, checksums, and public
installation evidence. Those later stages are not established by a source merge.

The default local gate defers native certification to independent checks.
Work Fabric A1 remains incomplete: surface agreement, restart recovery, and its
acceptance gate are outstanding in the
[task ledger](../ledgers/2026-08-22-work-fabric-task-ledger.md).
The separate macOS desktop rewrite is outside this promotion and was preserved
with a verified private snapshot. This checkpoint does not establish installed
runtime behavior, live provider entitlement, or a published 0.3.0 binary release.

## Integration follow-through

The follow-through synchronizes production history back into `dev` and retains
this receipt. It also keeps both `dev` and `docs` extras in the local gate's docs
build: requesting only `docs` removed Pytest from the shared `.venv`, causing a
subsequent local verification run to fail its Python and product tests.
