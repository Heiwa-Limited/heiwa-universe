# Desktop continuity and installed-main checkpoint

Classification: Intake, Execution, Evidence. Verified 2026-09-12.

## Delivered behavior

- Durable standalone and project sessions: create, rename, move, archive, restore,
  and reopen through the Rust session journal without changing execution IDs.
- One composer across Home, Calendar, Mail, and the conversation. Per-session
  drafts survive navigation; the anchored response panel preserves the surface.
- Fresh-profile identity and workspace setup independent of provider readiness,
  passive resource discovery, and explicit bounded Apple Mail header import.
- Session refresh repairs prevent older responses replacing acknowledged edits,
  release externally archived selections, retain observed sessions omitted by a
  bounded catalog, and clear a disposed conversation projection.

## Source and review

- [PR #99](https://github.com/Heiwa-Limited/heiwa-universe/pull/99): reviewed head
  `731f2593a11d75f01ab14968e5695bb450aed12f`, integrated at
  `719c6bb4239bcd4c150d856875e4e536f9a1dfeb`.
- [PR #100](https://github.com/Heiwa-Limited/heiwa-universe/pull/100): promoted to
  main at `2d7747d2ed17788c20b8caf7679748aff225b9c8`.
- Both PRs had all 11 remote checks passing and no unresolved review threads
  immediately before their exact-head merges. The integrated and main source
  trees match the reviewed checkpoint.

## Verification and machine installation

All 31 local checks passed on the clean experimental head and again on clean
`dev`, including the full Rust workspace suite, Clippy, security, and L0–L2.
Additional checks passed: 119 frontend tests, 9 packaging tests, 54 native tests
(with one isolated helper ignored in the normal harness), native Clippy, web
production build, native bundle build, and ad-hoc signature verification.

Five session regressions failed before their fixes. The packaged runtime passed
create/rename/move/restart/archive/restore in a disposable profile; no duplicate
session or project appeared. Probe processes and the fixture were removed.

A clean checkout was fetched at the exact GitHub main revision above, its coherent
runtime/native bundle was rebuilt, and `heiwa app update --source checkout --json`
installed it. The selected SDK 27 clang and stripping settings were carried into
the checkout installer. Active task workers and pending approvals were zero at
the update boundary. The existing app, CLI, and launcher configuration were backed
up before replacement; durable state was preserved.

Installed CLI and bundled-runtime SHA-256:
`41b0301b57475715f58312da5a4eb520b4beb0ccc5a2ca811e905a5c4088aced`.
Installed native executable SHA-256:
`c34ca48982561abd66c195878ec5f2ae63e7296cb5f837f66806410855586deb`.
Both matched the freshly built bundle. The existing LaunchAgent restarted the
installed CLI; runtime 7474 returned JSON health and the existing catalog was
preserved. Native UI verification confirmed draft continuity through Home,
Calendar, and Mail, an anchored response over Mail, and an empty composer on Home
after clearing the unsent test draft. No inference or Mail content read was run.

The private installation receipt records the source, hashes, process transition,
health, and rollback path. The native app is left open on Home for normal use.
Original peer worktrees and preexisting operator edits remain preserved.

## Limits

This is a local development installation from verified GitHub main. It does not
publish a new GitHub Release or establish Developer ID notarization, provider
execution, structured current-view context transport, Mail body/send support, or
complete Work Fabric A1 acceptance. Those need their own implementation and proof.


## Lifecycle repair found during delivery verification

A forced-stop test exposed an unowned `caffeinate -dimsu` process after a runtime
was killed. Its inherited pipe kept a concurrent Calendar fixture waiting for
EOF. The exact helper was traced through its pipe descriptors and removed.

The runtime now starts `caffeinate` with `-w` bound to its own PID, retaining normal
explicit cleanup while also ending the assertion after a crash or forced stop.
A macOS integration regression starts a real isolated runtime/helper, force-stops
the runtime, and requires the helper to exit. It failed before the fix and passed
afterward; the four app API tests and seven Calendar connector tests pass together.
The test owns and cleans only its own processes and temporary profile.
