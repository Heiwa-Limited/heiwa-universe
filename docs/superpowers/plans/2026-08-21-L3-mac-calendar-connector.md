# L3 Mac Calendar Connector Implementation Plan

> **For agentic workers:** Execute inline with test-driven development. Tasks are
> sized to complete user value, not to artificial patch size.

**Goal:** Let Heiwa.app on this Mac discover writable Apple calendars, stage a
specific event write for approval, execute it idempotently through Calendar.app,
and replay the resulting receipt from the local evidence journal.

**Architecture:** Extend the existing `schedule -> dispatch approval -> calendar
hold` authority path. A focused Apple bridge service owns Calendar.app JXA;
surface handlers only select policy and presentation. Connector state remains
under the resolved per-machine config root, credentials/Automation permission
stay on this Mac, and emitted domain records carry `work_id` plus the pre-mesh
`origin_device_id` without treating `device_id` as mesh node identity.

**Tech Stack:** Rust, Calendar.app JXA through `osascript`, local JSON receipts,
`heiwa_evidence` JSONL replay, Solid/TypeScript cockpit.

---

### Task 1: Prove the connector contract red

**Files:**
- Create: `apps/heiwa_shell/tests/apple_calendar_connector.rs`
- Modify: `apps/heiwa_shell/tests/schedule.rs`

- [ ] Add a hermetic `osascript` fixture and assert writable Apple resource
      discovery through the real `heiwa` binary.
- [ ] Stage `heiwa schedule ... --promote apple --calendar Calendar` and assert
      `work_id`, T2 promotion details, and no external write before approval.
- [ ] Approve the request and assert the external ID appears in the hold, the
      connector receipt file, the decision, and replay of
      `connector_receipts.jsonl`.
- [ ] Run the focused tests and observe failures caused by the absent commands
      and promotion mechanics.

### Task 2: Build the Mac-owned connector authority path

**Files:**
- Create: `apps/heiwa_shell/src/cmd/calendar_apple.rs`
- Modify: `apps/heiwa_shell/src/cmd/mod.rs`
- Modify: `apps/heiwa_shell/src/cmd/calendar.rs`
- Modify: `apps/heiwa_shell/src/cmd/schedule.rs`
- Modify: `apps/heiwa_shell/src/cmd/approvals.rs`

- [ ] Add exact-name writable-calendar discovery and a retry-safe event-create
      operation keyed by `heiwa://calendar/holds/<hold_id>`.
- [ ] Add `heiwa calendar calendars --source apple --json` and expose OS-owned
      authorization/revocation guidance.
- [ ] Add explicit `--promote apple --calendar <exact name>` staging; preserve
      existing local-only behavior when absent.
- [ ] Carry `work_id` through hold, approval, applied effect, file receipt, and
      append-only connector receipt journal.
- [ ] Keep external creation before local confirmation so a failed connector
      never marks a local hold completed; use the stable marker for safe retry.
- [ ] Add normalized attention fields to calendar read-model events.
- [ ] Run focused tests until green, then run the existing schedule, approval,
      and calendar-sync suites.

### Task 3: Make the value available in Heiwa.app

**Files:**
- Modify: `apps/heiwa_shell/src/cmd/app.rs`
- Modify: `apps/heiwa_app/clients/cockpit/src/lib/types.ts`
- Modify: `apps/heiwa_app/clients/cockpit/src/lib/endpoints.ts`
- Modify: `apps/heiwa_app/clients/cockpit/src/routes/Calendar.tsx`

- [ ] Expose a read-only Apple calendar resource endpoint.
- [ ] Let the existing authenticated hold endpoint optionally stage a named
      Apple promotion and return its approval request.
- [ ] Let the Calendar form self-discover writable calendars and clearly choose
      between local hold and approval-staged Apple event.
- [ ] Typecheck and build the cockpit.

### Task 4: Verify, record, and ship the full value path

**Files:**
- Modify: `docs/superpowers/specs/2026-08-18-L3-calendar-mail-connectors.md`
- Modify: `docs/superpowers/ledgers/2026-08-18-L3-connector-task-ledger.md`

- [ ] Run focused Rust tests, shell integration tests, web typecheck/build,
      formatting/static checks, `check_ci_local.sh`, and agent baseline.
- [ ] Verify checkout behavior with disposable state/evidence on `7475`; stop
      the temporary runtime and remove its fixtures.
- [ ] Stage and approve one clearly named live Calendar.app verification event,
      replay its connector receipt, verify by exact marker/external ID, then
      remove only that verification event.
- [ ] Update the L3 contract and ledger with exact evidence; keep Google and
      Gmail live lanes honestly separate.
- [ ] Commit on `dev`, push, promote through the protected `dev -> main` PR,
      update the installed runtime from checkout, and verify installed `7474`.
