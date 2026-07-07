# Heiwa.app Life UX — Calendar, Mail, and Invisible Model Routing

Date: 2026-06-11
Classification: Intake + Execution + Evidence product direction
Status: implementation spec seed

## Decision

The next Heiwa.app product push should move from developer cockpit toward a normal-user life operating surface:

1. **Heiwa Calendar** — a first-class local calendar read model that can sync Google Calendar and Apple Calendar into one user-facing schedule, conflict map, and planning surface.
2. **Communications / Mail** — a local-first inbox triage and reply surface that can scan, summarize, report, draft, and approval-send replies without turning email into a hidden automation risk.
3. **No model picker** — users should not choose Claude/Gemini/Codex/Ollama/model IDs in the main UX. Heiwa should route through an evolving model/provider/device evaluation matrix and show only the useful rationale: why this lane was chosen, privacy/cost class, and evidence.
4. **One conversation + inspectors** — Today, Calendar, Mail, Inbox, Approvals, Evidence, Providers, and Model Matrix are inspectors over the same runtime truth, not separate dashboards the user must manually coordinate.

This is not a hosted SaaS pivot. `Heiwa.app` and `heiwa` still run solely on user devices. Cloud surfaces remain support/evidence/distribution only.

## Official source constraints

### Google Calendar

Source: <https://developers.google.com/workspace/calendar/api/guides/sync>

Google Calendar supports efficient incremental sync:

- Initial full sync returns `nextSyncToken`.
- Incremental sync sends `syncToken` and receives changes since the last sync.
- Deleted entries are included so local stores can remove events.
- Paginated sync only returns the next sync token on the final page.
- `410 Gone` means the sync token is invalid; the client must wipe local synced state for that collection and do a new full sync.

Heiwa implication: store Google calendar sync tokens per account/calendar in local state and record receipt events for full sync, incremental sync, 410 recovery, and writes.

### Gmail

Source: <https://developers.google.com/workspace/gmail/api/guides/push>

Gmail push notifications use Cloud Pub/Sub and `users.watch`; history IDs are then resolved through `history.list`. Watch must be renewed at least every 7 days, with Google recommending daily renewal.

Heiwa implication: because Heiwa does not provide hosted app/runtime service, the default Gmail lane should be local pull/scheduled sync through OAuth. Pub/Sub can be an optional user-owned/project-owned enhancement later, not the first product dependency.

### Apple Calendar

Source: <https://developer.apple.com/documentation/eventkit>

EventKit provides access to calendar/reminder data, including create/retrieve/edit, recurrence, alarms, reminders, and change notifications when the Calendar database changes outside the app.

Heiwa implication: Apple Calendar should be a local macOS connector using EventKit permissions and local change notifications. It belongs in the device-local runtime/helper layer, not a cloud connector.

### Apple Mail

Source: <https://developer.apple.com/documentation/mailkit>

MailKit lets a macOS app include Mail extensions for content blocking, message actions, compose session handling, and message security. Compose session handlers can validate recipients and add custom headers.

Heiwa implication: Apple Mail should start as metadata/read-model and draft/approval UX. Product-grade Mail.app integration can later use a MailKit extension for compose/send safety, but Heiwa should not pretend MailKit is a general mailbox API.

## Product shape: one Life OS surface

### New primary app surfaces

The current nav has `Dashboard`, `Today`, `Inbox`, `Routes`, `Live`, `REPL`, `Providers`, `Connections`, `Status`. For normal-user UX, promote these concepts:

| Surface        | Purpose                                                                                      | Current mapping                                          |
| -------------- | -------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| `Today`        | Human daily brief: schedule, commitments, priority mail, approvals, stale facts, next blocks | Existing `Today.tsx`, needs real calendar/mail cards     |
| `Calendar`     | Unified Google + Apple event timeline, conflicts, holds, travel/lead-time, scheduling drafts | New route/read model                                     |
| `Mail`         | Priority email scan, thread summaries, draft replies, send approvals, receipts               | New route/read model; current `mail.rs` is metadata-only |
| `Intake`       | One stream of mail/calendar/messages/files/runtime alerts that need attention                | Evolves existing `Inbox.tsx`                             |
| `Work`         | Current mission/conversation stream, execution status, artifacts                             | Dashboard/Live/Repl converge here                        |
| `Approvals`    | Calendar writes, replies, messages, file/publish actions staged by risk                      | Existing Approvals route                                 |
| `Receipts`     | Evidence/citations/source refs for all reads/writes                                          | History/Traces merge visually                            |
| `Model Matrix` | Inspector-only: route health, eval scores, privacy/cost lanes; not a picker                  | Replace user-facing model choice language                |

### Interaction contract

The user should experience this as:

> “Heiwa knows my day, watches what changed, stages what needs action, handles safe work, and proves what happened.”

Concrete UX flow:

1. User opens Heiwa.app.
2. `Today` shows:
   - next 12 hours from Heiwa Calendar
   - conflicts / missing buffers / commitments
   - priority mail requiring response
   - suggested focus block
   - pending approvals
   - stale inputs
3. User says: “Clear my morning and reply to anything urgent.”
4. Heiwa:
   - reads local Calendar/Mail read models
   - classifies events and threads
   - proposes schedule changes and draft replies
   - stages writes as approvals
   - routes models invisibly based on matrix/evals
5. User approves/edits/rejects in one place.
6. Heiwa writes through connector leases and records receipts.

## Heiwa Calendar architecture

### Unified calendar record

Add a local read model shaped like:

```json
{
  "event_id": "heiwa-cal-...",
  "source": {
    "provider": "google_calendar|apple_eventkit|manual|heiwa_hold",
    "account_id": "...",
    "calendar_id": "...",
    "external_id": "...",
    "etag_or_version": "..."
  },
  "title": "Dentist",
  "start": "2026-06-12T10:00:00-07:00",
  "end": "2026-06-12T11:00:00-07:00",
  "timezone": "America/Vancouver",
  "location": null,
  "attendees": [],
  "visibility": "private|busy|free|unknown",
  "sensitivity": "low|personal|private|restricted",
  "status": "confirmed|tentative|cancelled|draft_hold",
  "freshness": {
    "synced_at": "...",
    "sync_kind": "full|incremental|local_change"
  },
  "receipt_refs": []
}
```

### Connector lanes

| Lane            | Auth                                       | Sync                                                                | Writes                                    | First product action               |
| --------------- | ------------------------------------------ | ------------------------------------------------------------------- | ----------------------------------------- | ---------------------------------- |
| Google Calendar | OAuth desktop flow, narrow Calendar scopes | full + incremental sync token; recover 410 by scoped wipe/full sync | create/update/delete after approval       | `calendar.list` + draft focus hold |
| Apple Calendar  | EventKit local permission                  | EventKit fetch + change notification                                | create/update via EventKit after approval | local day/week read model          |
| Heiwa Holds     | local runtime state                        | immediate local                                                     | promote to provider after approval        | focus blocks / travel buffers      |

### UX innovation

Heiwa Calendar is not “another calendar app.” It should expose:

- **Commitment map** — fixed obligations, soft holds, deadlines, errands, energy windows.
- **Attention pressure** — mail + calendar + approvals combined into “what is actually demanding time.”
- **Negotiation drafts** — when conflicts happen, draft reschedule emails/messages before moving events.
- **Receipted schedule changes** — every external calendar write has before/after, external event ID, account, approval ID, and undo posture.

## Mail / communications architecture

### Local mail read model

Add a message/thread read model shaped like:

```json
{
  "thread_id": "heiwa-mail-thread-...",
  "source": {
    "provider": "gmail|apple_mail|imap",
    "account_id": "...",
    "external_thread_id": "...",
    "external_message_id": "..."
  },
  "from": "redacted-or-header",
  "to": ["redacted-or-header"],
  "subject": "...",
  "date": "...",
  "snippet": "...",
  "labels": ["INBOX", "UNREAD"],
  "priority": "low|normal|high|urgent",
  "requires_response": true,
  "summary": "...",
  "suggested_action": "ignore|archive|reply|schedule|approve",
  "sensitivity": "low|personal|private|restricted",
  "draft_reply_id": null,
  "receipt_refs": []
}
```

### Connector lanes

| Lane          | Auth                                                                         | Sync/read                                                     | Writes                                    | First product action      |
| ------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------- | ----------------------------------------- | ------------------------- |
| Gmail         | OAuth desktop flow, Gmail read/send scopes split                             | local scheduled search/list/get/history where possible        | draft reply + approval send via Gmail API | daily priority scan       |
| Apple Mail    | local metadata probe now; MailKit extension later for compose/session safety | metadata first; body only after explicit connector permission | open/stage draft locally; MailKit later   | inbox report from headers |
| IMAP/Himalaya | user-owned IMAP/SMTP config                                                  | folder/envelope/message read                                  | template reply/send after approval        | fallback non-Gmail lane   |

### Reply safety

All outbound mail should be staged by default:

1. `draft_proposed` — model drafts but cannot send.
2. `approval_requested` — user sees recipients, subject, body, attachments, source thread, risk class.
3. `send_approved` — connector lease executes send.
4. `send_receipt` — external ID/thread ID, timestamp, approval ID, account, body hash, redaction state.

No background “just reply” without an approval policy that is explicitly configured and receipt-backed.

## Invisible model routing / evolving matrix

### User-facing rule

There is no normal-user model picker in Heiwa.app.

The app may show:

- “Local/private lane used”
- “Escalated to code specialist”
- “Used long-context lane for 19 emails”
- “Cost: $0 incremental / subscription / metered”
- “Why: calendar extraction + low risk + local sufficient”

It should not ask:

- “Choose GPT-5.5 vs Claude vs Gemini”
- “Pick temperature/top_p”
- “Select model ID for this email”

### Internal matrix inputs

| Input            | Examples                                                               |
| ---------------- | ---------------------------------------------------------------------- |
| Task class       | summarize thread, draft reply, schedule conflict, code edit, UI design |
| Risk class       | read-only, draft-only, external send, calendar write, destructive      |
| Data sensitivity | local-only, personal, private, restricted                              |
| Context shape    | short, long, tabular, code, calendar, thread, attachment               |
| Device state     | battery, CPU, memory, local model availability                         |
| Provider state   | connected, quota, latency, errors, auth mode                           |
| Eval score       | own regression fixtures per task class                                 |
| Cost             | local/free/subscription/metered                                        |
| Evidence quality | can cite source spans, message IDs, event IDs, receipts                |

### Matrix UI

Model Matrix should be an inspector route, not a chooser:

- lane health cards: local, subscription CLI, provider agent, metered API
- task-class score table: summarize, draft, route, code, vision, calendar, mail
- recent routing decisions with receipts
- eval regressions and stale model/provider data
- “override for this task” only in advanced mode, always receipt-backed

## First implementation slices

### Slice 1 — UX read-model spec in app

Plane: Intake/Evidence

- Add `Calendar` and `Mail` routes to cockpit nav.
- Render placeholder cards from explicit local JSON endpoints or static fixtures under runtime state.
- Make Today visually reserve slots for schedule pressure and priority mail.
- No external account writes.

Acceptance:

- `npm run typecheck && npm run build` passes.
- Cockpit shows Calendar/Mail as first-class surfaces.
- Empty-state copy is honest: “connector not enabled yet.”

### Slice 2 — Runtime contracts

Plane: Intake/Evidence

- Add `/api/v1/calendar/summary` and `/api/v1/mail/summary` read endpoints.
- Add tests asserting JSON schema shape and no body/secret leakage.
- Use local state files first:
  - `~/.heiwa/state/calendar/events.jsonl`
  - `~/.heiwa/state/mail/threads.jsonl`

Acceptance:

- endpoints return valid envelopes even with no connectors
- Inbox can merge mail/calendar items into typed `InboxItem`
- tests fail first, then pass

### Slice 3 — Apple Calendar local bridge

Plane: Intake

- macOS helper/command uses EventKit with user permission.
- Fetch calendars/events into local read model.
- Listen for local calendar database change notifications where practical.
- No writes until approvals and receipts exist.

Acceptance:

- `heiwa calendar status --json`
- `heiwa calendar sync --source apple --dry-run --json`
- local receipt for read/sync

### Slice 4 — Google Calendar connector

Plane: Intake/Evidence

- OAuth setup with narrow Calendar scopes.
- Full sync then incremental sync with persisted `nextSyncToken`.
- 410 recovery path records evidence before wiping scoped local synced rows.

Acceptance:

- `heiwa calendar sync --source google --json`
- sync token persisted per account/calendar
- external event IDs stored but sensitive fields redacted according to policy

### Slice 5 — Gmail priority scan + draft replies

Plane: Intake/Execution/Evidence

- OAuth setup with read scope first; send scope gated separately.
- Pull-based scan by default because Heiwa has no hosted webhook service.
- Priority classifier can use local/private lane first.
- Draft replies are staged as approvals.

Acceptance:

- `heiwa mail scan --source gmail --json`
- `heiwa mail draft-reply THREAD_ID --json`
- no send without approval

### Slice 6 — Model Matrix route inspector

Plane: Execution/Evidence

- Replace user-facing model picker language with lane/matrix health.
- Add task-class eval metadata and recent routing decisions.
- Keep provider detail visible for debugging, not normal choice.

Acceptance:

- `Providers` remains connection state.
- `Model Matrix` shows routing quality/availability.
- `route preview` can explain “why this lane” without exposing model-picking burden.

## App visual direction

Heiwa.app should move from “admin cockpit” to “life ops console”:

- Left rail: Today, Calendar, Mail, Intake, Work, Approvals, Receipts, Model Matrix, Settings.
- Center: one conversational/action stream with cards for schedule, messages, approvals, artifacts.
- Right inspector: evidence, connector scopes, source refs, route rationale.
- Bottom composer: single user input with staged action chips.
- Status footer: local runtime, STDB sync, device load, connector freshness, matrix health.

The user should see fewer nouns, not more. The system can be complex internally; the app must compress it into a calm operating layer.

## Competitive wedge

Peers sell integrations or model access. Heiwa should sell **governed life execution**:

- local-first device authority
- cross-calendar/mail context
- staged writes and replies
- invisible routing to the right model/tool
- receipts and source spans
- no hosted app runtime dependency
- no “choose your model” cognitive tax

That is the competitive story: not a prettier chat app, not a calendar clone, not a mail client clone — a device-local operator that understands time, obligations, communications, tools, and evidence.
