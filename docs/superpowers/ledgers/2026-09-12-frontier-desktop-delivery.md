# Frontier desktop delivery

Classification: Intake, Execution, Evidence. Started 2026-09-12 from GitHub dev
`414763e087a09c31d090df6289bb810c4a222820`.

The user authorizes implementation, normal protected-branch merges, public GitHub
distribution, and installation on this machine. Existing worktrees, edits, user
data, credentials, and active work must be preserved. This ledger records actual
delivery; it does not redefine unfinished product capabilities as complete.

| Checkpoint | State | Acceptance |
| --- | --- | --- |
| Reconcile remaining branches and lifecycle patch | In progress | Determine unique code versus already merged work; test recovered observations and runtime ownership |
| First-run workspace and resource connections | In progress | Fresh profiles, in-app provider setup, scoped resource selection, actual data load, disconnect/restart/isolation |
| Rich desktop work | Pending | Selected-object context, attachments, provider translation, persisted artifacts, efficient conversation and workspace views |
| Combined app/runtime/CLI distribution | Pending | Matching packaged bytes, no checkout/toolchain requirement, profile identity, update safety, signed/notarized browser install |
| Integration, public delivery, local installation | Pending | Current-head local gates, PR checks/review, main certification, GitHub artifacts, install and data readback receipts |

## Reconciliation evidence

The installed development checkpoint is main `38825bbc`; runtime 7474 responds,
with no task workers or pending approvals at the initial read-only probe. The
public latest release is still v0.2.0. The local CLI reports 0.3.0.

Already ancestors of current dev: A1-c2, runner recovery, Codex stream repair,
price truth, publishing consistency, successful checkpoint, and worker evidence.
The old macOS onboarding worktree contains retained uncommitted work that must
be compared with the already delivered session/onboarding checkpoint.

Still unique: the uncommitted desktop lifecycle patch (carried into this branch),
two provider-truth design/plan commits, one continuity architecture commit, and
open PR #87's executable claim registry. Older green checks on #87 are stale;
its last Rust check failed. These are not to be merged merely because they exist.

## External distribution dependency

The initial signing probe found zero valid code-signing identities. GitHub
exposes a Tauri updater signing secret but no Apple signing/notarization secret
names. Existing Apple Developer setup was requested from the user. Developer ID
and notarization remain prerequisites to claiming normal browser-download
Gatekeeper usability; updater signatures establish a different trust boundary.

## References

Use the pinned source analysis in
`docs/research/2026-09-12-desktop-reference-extraction.md`. Current provider setup
documentation was refreshed at [OpenCode](https://opencode.ai/docs/providers)
and [Goose](https://block.github.io/goose/). Apple distribution requirements were
refreshed at [Developer ID](https://developer.apple.com/developer-id/) and
[notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).

## Implementation under verification

- Restored explicit observation recovery and application-level runtime ownership.
- API connection setup shares the provider service with the CLI. Registry writes
  are atomic and merge nonconflicting account changes; stale probes cannot revive
  removed accounts. OpenRouter verification now checks the authenticated key
  endpoint before admitting catalog models. Cached API accounts appear in the
  runtime projection without invented validation times.
- Desktop startup provisions the packaged CLI atomically. The updater checks
  observed activity before and after download and defers for separately owned
  runtimes. This is not yet an atomic admission barrier for every new request.
- Swift EventKit reads selected calendar identities and recurring occurrences.
  Rust persists selection, reconciles complete snapshots, preserves unseen rows
  on truncation, and records bounded read receipts. Mail and Calendar reuse the
  same bounded subprocess lifecycle. Real installed-app permission/readback
  verification remains required.
- The release pipeline now prepares Developer ID signing, notarized DMG/update
  artifacts, matching published/bundled CLI bytes, and credential cleanup on
  ephemeral runners. This workflow has not published a new release.

Current remote PR #87 head is `32a7c3434217407d95bfa480d8cf1417664ec450`
(September 1 merge of dev), ahead of the retained local worktree's older head.
Its Rust tests and aggregate status failed. Reconciliation must use that remote
head and current dev, not the older local tip.

Targeted verification so far: provider unit suite 122 passed / 3 OS-vault tests
ignored; shell unit suite 238 passed; native suite 56 passed / 1 external-runtime
test ignored before the shared subprocess extraction; installer suite 9 passed;
desktop tests include explicit calendar-selection/import coverage. Swift reader
compiled with the macOS 27 SDK. These are development checks, not final clean
source, release, or installation receipts. An initial full-gate run exposed a
production dependency declaration and Clippy/format issues; those were repaired
and require a fresh full-gate run.

Retained onboarding worktree comparison: its modified code is already in dev;
the remaining differences are newer dev fixes (clearing disposed observations,
crash-bound keep-awake) and corrected GitHub/Cloudflare documentation. No unique
production implementation was found in that retained tracked diff. It remains
preserved in place.

Calendar end-to-end fixture verification passes through the real CLI/service:
unenrolled reads make no source call; explicit connect/import persists events;
rename keeps identity; unselected rows are rejected; truncation preserves unseen
rows; complete refresh removes deleted rows; disconnect prevents later reads.
No personal Calendar data was used by these tests.

GitHub's current hosted runner inventory tops out at macOS 26. The application
still targets macOS 27. Hosted build/signature checks must not be described as
macOS 27 first-launch certification; the installed Mac and a fresh-profile
exercise supply that separate proof.
