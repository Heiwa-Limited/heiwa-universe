# L3 — Calendar and Mail Connectors

Date: 2026-08-18
Status: Draft — implementation-ready except where marked
Plane: Intake + Execution + Evidence
Depends on: `2026-08-18-build-foundation.md` (Phase 1), AD-14, `docs/references/google-oauth-native.md`

## Purpose

Make the approval and receipt plane load-bearing. Every write crosses
`AwaitingApproval` and lands a receipt that replays from the journal. Until a
connector executes under it, the trust plane is architecture nobody has used.

## What already exists

Verified 2026-08-18, so the spec builds on the real seam rather than a guess:

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

**Not present:** any authorization-code flow. Grepping for `code_challenge`,
PKCE, or a loopback listener returns nothing. The flow itself is net-new; the
storage it feeds is not.

## Scope selection — no restricted scopes

Google classifies Gmail's read scopes as restricted, which pulls in an annual
third-party security assessment. This design uses none of them.

| Capability | Mechanism | Google tier |
|---|---|---|
| Read calendar | `calendar.readonly` | sensitive |
| Write calendar | `calendar.events` | sensitive |
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
- **AD-17 — The OAuth client id ships in the binary and is not treated as a
  secret.** Native apps are public clients; Google issues a "client secret" for
  desktop clients that cannot be kept secret in a distributed binary, and the
  loopback flow does not require it. PKCE is what makes the exchange safe. Any
  design that depends on that value staying private is wrong.
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

## Build order

1. `heiwa_oauth` — loopback + PKCE + exchange + refresh, against a mock server.
   No Google dependency, fully testable in CI.
2. Token storage through `heiwa_vault`, reusing `needs_refresh`.
3. Google Calendar read → the Calendar surface renders live events beside the
   local ones, with the source labelled.
4. Calendar write → `AwaitingApproval` → receipt → journal replay. **This is
   the milestone that makes the moat real.**
5. `gmail.send` on the same path.

Steps 1 and 2 need no credential and no external account. Step 3 onward needs
the client id below.

## Blocked on Devon

**Create a Google Cloud project and an OAuth client of type "Desktop app".**
Nothing in steps 3–5 can be tested without it, and it cannot be automated —
it requires an account, a consent screen, and accepting Google's terms.

What is needed back: the client id, and the project kept in testing mode with
Devon's account as a test user. No verification submission yet; that is a
distribution task for when the app ships publicly, and the exemption covers
development.

## Verification

L3 is complete when a calendar write executes against a live account under
approval and the resulting receipt replays from the journal — the criterion
already set in the 2026-08-14 roadmap. `heiwa_oauth` carries its own unit
coverage against a mock authorization server, so the flow is proven in CI
without a network or an account.

## References

- `docs/references/google-oauth-native.md`
- `docs/superpowers/specs/2026-08-18-build-foundation.md`
- `docs/superpowers/specs/2026-08-14-heiwa-app-product-roadmap-design.md`
