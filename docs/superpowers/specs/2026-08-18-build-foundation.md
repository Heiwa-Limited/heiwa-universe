# Heiwa Build Foundation

Date: 2026-08-18
Status: Active
Plane: Intake + Execution + Evidence
Supersedes sequencing in: `2026-08-14-heiwa-app-product-roadmap-design.md` (L3 detail only)

## Summary

Everything measured on 2026-08-18, against a live machine and live edges. This document
records what is verified true, the two findings that change the plan, and the sequence that
follows from them.

The headline: **Heiwa's distribution loop is sealed at both ends, and its one rare asset —
the approval and receipt plane — is not yet load-bearing.** Both are fixable without a
purchase, a credential, or a hosted service.

## Verified state

| Measure | Value |
|---|---|
| Product code | 148,222 LOC / 799 files, 0 unclassified |
| Rust runtime | ~70k (shell 33k, crates 29k, core 7.5k) |
| Desktop shell | 4,983 LOC TypeScript |
| Peak velocity | 109 commits/week; L0→L2 spec-to-shipped in ~2 days |
| Executable connectors | **0** (1 validated manifest) |

2026-08-21 update: the executable connector count is now **1**. Apple Calendar
has real resource discovery, read, T2 event creation, receipt replay, and
OS-owned revocation guidance. The table above remains the measured 2026-08-18
baseline.

Verified working, by execution rather than by document:

- **Approval gate** — `heiwa approvals list` returns 3 pending requests with
  `risk=critical` classification and 1 decision on record. `ApprovalVerdict::AwaitingApproval`
  in `crates/heiwa_drex/src/drex_gate.rs`, with tests.
- **Receipts** — SHA-256 hash chain (`prev_hash`/`entry_hash`, `verify_chain`,
  `migrations/0002_hash_chain.sql`). 106 events across 9 journal streams.
- **Routing** — a live DREX decision with quota across 4 rate groups, reason
  `lowest_known_marginal_cost_then_quality_latency_success`. Reports `stage: legacy_route`;
  the full DREX stage is not yet the one executing.
- **BYOK** — `fresh_install.rs` completes a turn with an emptied `PATH` and one API key.
- **Local-first reads** — Calendar and Mail snapshots under the config root, sourced from
  this machine, no cloud.

Verified *not* working:

- **Subagent fan-out is a planner with no executor.** `crates/heiwa_loop/src/fanout.rs`
  states it outright; the only consumer in the tree is `lib.rs` re-exporting the types.
  It must not be cited as a shipping differentiator.
- **Browser surface is an iframe.** L4 as designed is unbuilt.

## Finding 1 — the distribution loop is sealed at both ends

v0.1.0 was released 2026-08-13 at 08:20. The working update path landed at 12:40 the same
day, four hours later. Therefore:

```
heiwa app update --dry-run --json
→ "implemented": false
→ "blocker": "GitHub release update awaits release asset verification"
```

The only binary anyone can install cannot update itself. The public installer that would
otherwise reinstall it is pinned to `0.1.0` at the edge — `resolve_latest_version` exists in
`apps/heiwa_app/clients/web/install` and is absent from what `heiwa.ltd/install` serves.

Every user who installs Heiwa is stranded on a dead version permanently. No improvement
downstream of this reaches anyone until it is cut.

`scripts/check_public_installer.sh` passes locally, so CI cannot see the drift. That gap is
part of the fix, not an aside.

## Finding 2 — the local-first mail bridge routes around Google's restricted-scope wall

Google classifies `gmail.readonly`, `gmail.metadata`, `gmail.compose`, `gmail.modify`, and
`mail.google.com` as **restricted**. Restricted scopes require an annual third-party
security assessment (CASA) by a Google-empanelled assessor for any app that can reach user
data through a third-party server. `gmail.send` is merely **sensitive**. Calendar is not on
the restricted list.

Reading a user's mail from **Mail.app on their own machine requires no Google scope at
all** — which is what `heiwa mail scan` already does.

This inverts the standing assumption that the local bridge is a placeholder awaiting "real"
cloud mail. The cloud path is the expensive one. The local path is the only mail-reading
capability a solo publisher can ship to strangers without a recurring paid audit.

Resulting L3 split — **no restricted scope is required to ship the executive assistant**:

| Capability | Path | Verification |
|---|---|---|
| Read mail | Mail.app bridge (built) | none |
| Send mail | `gmail.send` | sensitive |
| Read calendar | Calendar.app bridge (built) + `calendar.readonly` | none / sensitive |
| Write calendar | `calendar.events` | sensitive |

Verification is additionally **waived entirely** while the app serves only the developer and
personal acquaintances, or is in development/testing. Build against real scopes now;
submission is a distribution task, not a prerequisite.

Full detail: `docs/references/google-oauth-native.md`.

## Finding 3 — the moat is dead weight until L3

Approvals and receipts add nothing to chat, because chat has no consequences. They pay off
exactly when an agent does something irreversible: sends the mail, moves the money, changes
the calendar.

So the one thing competitors structurally lack — ChatGPT and Claude cannot run local-first
with the user's own logins; T3 Code and Odysseus have no approval or audit layer — sits
unused until a connector executes under it.

This is a favourable position, not a bad one. The rare part is architectural and slow to
add. The missing part (OAuth) is commodity and fast. The reverse would be fatal.

## Sequence

### Phase 1 — unseal distribution

Days. No purchase, no credential, no hosted service.

1. Ship `Heiwa.app` inside the release tarball the installer already downloads and
   checksums. `release.yml` builds `dmg,app` and its macOS artifact glob captures only
   `*.dmg`, discarding the `.app`.
2. Redeploy the edge installer so `resolve_latest_version` is live, and add an
   edge-versus-repo drift check so CI can see this class of failure.
3. Wire `tauri-plugin-updater` with a free minisign keypair, publishing the signed manifest
   from `release.yml` alongside the existing checksum manifest.
4. Cut v0.2.0 carrying the working `heiwa app update`.

Do not publish a `.dmg` as the headline artifact. Browser downloads set the quarantine bit;
`curl` and the updater do not. Detail: `docs/references/tauri-updater.md`.

### Phase 2 — one connector, all the way through — complete 2026-08-21

Mac Calendar.app resource discovery → local read model → exact T2 event write,
where the write crosses approval and lands a receipt that replays from the
journal. Google OAuth remains the portable Calendar/Gmail expansion path; it
does not gate the Mac-first product lane.

This is the first moment the trust plane does work, and the first capability ChatGPT cannot
match on architecture rather than on polish.

### Phase 3 — the loop that runs while the app is open

`heiwa auto status` reports 0 automations, 0 cron jobs, 0 file watchers against a built
executor, scheduler, and storage. One default automation — a morning brief from real
calendar and mail, with anything actionable queued as an approval — converts "an app you
open" into "an app that was working before you opened it."

### Phase 4 — breadth on a proven plane

Further connectors, subagent executor, browser surface. Only after 1–3.

## Cut

- **Hide the placeholder surfaces** until each has a connector behind it. Finance, Social,
  Files, and Browser rendering "pending" is what makes a shipped app read as a prototype.
- **Stop citing subagents as a differentiator** until `fanout.rs` has an executor.
- **D1 / cross-device sync** — blocked, off the critical path, decision can wait.
- **Worktree management** — T3 Code's ground, for developers. Chasing it reverses AD-14.
- **Apple Developer Program** — not required for auto-updating distribution. Declined.

## Verification

Phase 1 is complete when a machine running v0.2.0 receives a subsequent release without
re-running the installer, and `scripts/check_public_installer_edge.sh` proves the edge
serves the repo's installer rather than a pinned copy.

Phase 2 passed on 2026-08-21: a live Calendar.app write executed under approval,
returned one external id, and replayed one connector receipt with zero skipped
lines. The exact verification event was removed by marker plus external id.

## References

- `docs/references/google-oauth-native.md`
- `docs/references/tauri-updater.md`
- `docs/superpowers/specs/2026-08-14-heiwa-app-product-roadmap-design.md`
- `docs/superpowers/ledgers/2026-08-14-L0-L1-task-ledger.md`
