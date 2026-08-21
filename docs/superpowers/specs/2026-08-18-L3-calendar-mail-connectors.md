# L3 — Calendar and Mail Connectors

Date: 2026-08-18
Status: Active — Mac-first Calendar acceptance complete; Google/Gmail expansion pending
Plane: Intake + Execution + Evidence
Depends on: `2026-08-18-build-foundation.md` (Phase 1), AD-14, `docs/references/google-oauth-native.md`

## Purpose

Make the approval and receipt plane load-bearing. Every write crosses
`AwaitingApproval` and lands a receipt that replays from the journal. Until a
connector executes under it, the trust plane is architecture nobody has used.

## What already exists

Verified through 2026-08-21, so the spec builds on the real seam rather than a guess:

- `heiwa_vault::Vault` — `store_oauth` / `load_oauth` over an `OAuthSecret`.
- `heiwa_provider::oauth::ProviderVault` — keychain-backed storage plus
  `needs_refresh(secret, now_unix, skew_seconds)`. Token lifecycle is solved.
- `heiwa_drex::drex_gate` — `ApprovalVerdict::AwaitingApproval { request_id,
  request_path }`, under test.
- `heiwa_a2a::RiskTier` — the classification vocabulary.
- `heiwa_receipts` — SHA-256 hash chain with `verify_chain`.
- `heiwa_automations` — executor, scheduler, storage.
- Local bridges — `heiwa calendar sync` and `heiwa mail scan` already read the
  user's own Calendar.app and Mail.app.
- `heiwa_oauth` — loopback PKCE, exchange, refresh, and strict listener
  deadlines, including a full-flow test against a local mock provider.
- `heiwa connect google-calendar` — reduces Google's downloaded desktop-app
  JSON to a versioned public client-id record, opens consent without logging
  the OAuth state URL, and stores OAuth tokens only through `heiwa_vault`.
- `heiwa calendar calendars --source apple` — discovers exact writable
  Calendar.app resources through macOS-owned Automation permission.
- `heiwa schedule ... --promote apple --calendar <exact name>` and the
  Heiwa.app Calendar form — stage a named T2 write; `approvals decide` is the
  only execution point.
- Apple event creation is retry-safe through a stable
  `heiwa://calendar/holds/<hold_id>` marker. The file receipt and
  `connector_receipts.jsonl` carry the same external id, `work_id`, approval
  id, pre-mesh device handle, and undo posture.

**Not yet live-proven:** Google Calendar read/write and Gmail send. The offline
protocol and storage seams are built; the account cannot enter Cloud Console
until 2-step verification is enabled, after which a Desktop OAuth client id is
still required.

## Scope selection — no restricted scopes

Google classifies Gmail's read scopes as restricted, which pulls in an annual
third-party security assessment. This design uses none of them.

| Capability | Mechanism | Google tier |
|---|---|---|
| Read calendar | Calendar.app bridge / `calendar.readonly` | none / sensitive |
| Write calendar | Calendar.app bridge / `calendar.events` | none / sensitive |
| Read mail | Mail.app bridge (built) | **no scope** |
| Send mail | `gmail.send` | sensitive |

Reading mail stays local. That is not a compromise pending "real" cloud mail —
it is the only mail-reading path shippable to strangers without a recurring
paid audit, and it is already built.

While the app serves only the developer and personal acquaintances, or is in
testing, verification is waived entirely. Build against real scopes now.

## Decisions

- **AD-16 — A new crate `crates/heiwa_oauth` owns the installed-app flow.**
  Not an extension of `heiwa_provider::oauth`: that module is about model
  provider credentials, and a connector is not a model provider. The flow
  itself (loopback listener, PKCE, code exchange, refresh) is provider-agnostic
  and takes its endpoints and scopes as parameters, so it is testable against a
  mock authorization server with no Google account involved.
- **AD-17 — The OAuth client id is node configuration and is not treated as a
  secret.** Native apps are public clients; Google issues a "client secret" for
  desktop clients that cannot be kept secret in a distributed binary, and the
  loopback flow does not require it. Heiwa discards that field from the
  downloaded JSON and persists only a versioned client-id record. PKCE is what
  makes the exchange safe. Any design that depends on that value staying
  private is wrong.
- **AD-18 — The loopback listener binds an ephemeral port, serves exactly one
  request, and shuts down.** A listener that outlives the exchange is an open
  local port any process on the machine can talk to. `state` is required and
  checked.
- **AD-19 — Risk tiers for connector actions.** Reads are automatic; anything
  the recipient can see is gated.

  | Action | Tier | Gate |
  |---|---|---|
  | read events, read free/busy | T0 | automatic |
  | create/modify an event on own calendar | T2 | approval required |
  | modify an event with other attendees | T2 | approval, attendee list shown |
  | send mail | T2 | approval, full body shown |
  | delete a calendar, ACL change | T3 | explicit broker, refused by default |

  Mail send shows the rendered body in the approval, never a summary. An
  approval the user cannot read is not an approval.
- **AD-27 — Mac-first Calendar.app is a complete connector lane, not a mock
  pending Google.** macOS owns Automation consent and revocation; Heiwa owns
  exact resource selection, T2 approval, execution, and evidence. Google
  remains a portable expansion lane rather than the gate on local value.
- **AD-28 — Connector domain records carry `work_id`; receipts name
  `origin_device_id`, not `node_id`.** `device_id` remains the unsigned
  pre-mesh local handle. L5 public-key node identity is not fabricated early.
- **AD-29 — Apple event creation is idempotent by stable hold URL.** A retry
  returns the one existing event only when title/start/end still match; marker
  collisions or changed content fail closed.
- **AD-30 — External creation precedes local confirmation.** A connector
  failure leaves the hold draft and writes no approval decision. Once the
  external side effect succeeds, its deterministic receipt id supports
  at-least-once evidence without ambiguous action identity.

## Build order

1. `heiwa_oauth` — loopback + PKCE + exchange + refresh, against a mock server.
   No Google dependency, fully testable in CI. **Complete.**
2. Token storage through `heiwa_vault`, reusing `needs_refresh`. **Complete, including the shell caller.**
3. Apple Calendar resource discovery and read model. **Complete.**
4. Apple Calendar write → T2 approval → external id → file receipt → journal
   replay. **Complete; this is the accepted L3 milestone.**
5. Google Calendar read/write on the same normalized read model. Offline
   caller complete; live account setup remains external.
6. `gmail.send` on the same approval/evidence path.

Steps 1–4 need no Google credential. Steps 5–6 need the client id below.

## Remaining Google setup

**Enable 2-step verification on Devon's Google account, then create a Google
Cloud project and an OAuth client of type "Desktop app".** Live inspection on
2026-08-20 confirmed that Google Cloud blocks the account before the console
until 2-step verification is enabled. This authentication change cannot be
completed by Heiwa; it requires Devon's direct confirmation in Google's UI.

What is needed back: the downloaded desktop-app JSON, with the project kept in
testing mode and Devon's account as a test user. `heiwa connect
google-calendar --client-secret <path>` extracts only the public client id; it
never persists Google's bundled client-secret field. No verification
submission yet; that is a distribution task for when the app ships publicly,
and the exemption covers development.

This does not block the accepted Mac-first L3 connector milestone.

## Verification

The roadmap's L3 criterion passed on 2026-08-21 through the live Mac
Calendar.app lane: one exact event was staged as T2, created only after
approval, returned one external id, and replayed as one connector receipt with
zero skipped lines. Cleanup matched both the stable marker and external id and
left zero verification events. CI replaces only Calendar.app with a hermetic
`osascript` fixture while exercising the real binary, app endpoint, approval,
state, file receipt, and journal replay.

Google Calendar and `gmail.send` remain incomplete connector breadth. They no
longer make the already-proven local connector milestone appear blocked.

## References

- `docs/references/google-oauth-native.md`
- `docs/superpowers/specs/2026-08-18-build-foundation.md`
- `docs/superpowers/specs/2026-08-14-heiwa-app-product-roadmap-design.md`
