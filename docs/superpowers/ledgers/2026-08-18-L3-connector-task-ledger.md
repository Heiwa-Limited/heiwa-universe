# L3 Connector Plane — Task Ledger

Contract: `docs/superpowers/specs/2026-08-18-L3-calendar-mail-connectors.md`
Started: 2026-08-18

Status is what is true at HEAD, not what is intended. A row moves to done only
when its verification runs.

## Steps

| # | Step | Status | Verification |
|---|---|---|---|
| 1 | `heiwa_oauth` — loopback + PKCE + exchange | **done** | 28 tests incl. end-to-end flow against a mock provider and deadline-bounded listener timeout; `cargo test -p heiwa_oauth` |
| 2 | Shell caller + token storage through `heiwa_vault` | **done** | Calendar caller uses `heiwa_oauth`; refresh-token preservation is tested; tokens exist only in the OS credential vault |
| 3 | Apple Calendar resource list + read model | **done** | exact writable resources discovered through Calendar.app; normalized source/deadline/actionable/privacy fields feed the read model |
| 4 | Calendar write under approval → receipt replay | **done** | 2026-08-21 live Calendar.app write returned one external id; T2 approval, `work_id`, file receipt, and one replayed journal event agreed; exact verification event removed |
| 5 | Heiwa.app Calendar staging | **done** | fresh profiles reveal no Calendar.app resources until explicitly enrolled; native desktop and cockpit connect/disconnect, stage an exact Apple target without creating, and decide the resulting immutable approval through the shared executor; connector integration tests + both TypeScript checks/builds |
| 6 | Google Calendar read/write | blocked (external setup) | offline caller path is wired; needs Google account 2-step verification and a Desktop OAuth client id for live acceptance |
| 7 | `gmail.send` on the same path | pending | needs Google setup plus an approval-backed sender; Gmail reads remain local through Mail.app |

Steps 1–5 establish the product-grade Mac-first connector without a Google
account. Google and Gmail breadth remain separate work rather than an L3
milestone blocker.

## External dependency

Steps 6–7 need a Google Cloud project with an OAuth client of type "Desktop
app", kept in testing mode with Devon's account as a test user. Live inspection
on 2026-08-20 reached Google Cloud's account gate: the account must enable
2-step verification before Cloud Console will open. That authentication change
requires Devon's direct confirmation in Google's UI. After that, create the
project/client and stage its downloaded JSON with `heiwa connect
google-calendar --client-secret <path>`; Heiwa persists only the public client
id.

No verification submission is needed while the app serves only the developer
and personal acquaintances, or is in testing. That exemption covers all
development work here.

## Decisions

AD-16 through AD-19 are recorded in the spec. Implementation added:

- **AD-20** `heiwa_oauth` takes its endpoints as fields rather than constants,
  matching AD-3's reason on the direct-API adapters: a flow that can only talk
  to Google cannot be proven in CI. The end-to-end test stands up a token
  endpoint locally and drives a real browser callback through the real
  listener.
- **AD-21** Refresh-token preservation is a named function with its own test
  rather than an inline fallback. A provider omitting `refresh_token` on
  refresh is normal; dropping it breaks renewal an hour later, far from the
  edit that caused it, and an inline `.or_else` is exactly the shape a later
  refactor deletes without noticing.
- **AD-22** The PKCE challenge is tested against RFC 7636's own worked
  example rather than a round-trip of our own encoder. A round-trip test
  passes even when the encoder pads, uses standard base64, or hashes the
  wrong bytes — all of which a provider rejects.
- **AD-23** A provider denial is reported as a denial even when the callback
  omits `state`. Providers do not reliably echo state on the error path, and
  a user who clicked "deny" should not be told their callback was corrupt.
- **AD-24** The shell caller uses `heiwa_oauth` plus `heiwa_vault`; it does not
  duplicate PKCE with shell commands or write token JSON beneath the config
  root. The downloaded desktop-app file is reduced to a versioned public
  client-id record under node state, with owner-only permissions.
- **AD-25** Connector status distinguishes absent credentials, a credential
  vault backend failure, absent client configuration, and malformed client
  configuration. A corrupt config is `config_error`, never silently rendered
  as first-time setup.
- **AD-26** No Gmail read scope or API caller exists. Mail reads remain the
  metadata-only Mail.app lane; `gmail.send` is advertised but cannot be granted
  until an approval-backed sender is implemented.
- **AD-27 through AD-30** Mac-first Calendar.app is a valid complete connector
  lane; domain records carry `work_id` without pretending `device_id` is a
  mesh node key; stable hold markers make external creation retry-safe; and
  the external write must succeed before the local hold becomes confirmed.

## 2026-08-21 Mac-first acceptance

- `cargo test -p heiwa-shell --test apple_calendar_connector` — 7/7, including
  disconnected fresh-profile resource privacy, authenticated app staging,
  app-side approval execution, disconnect-before-approval enforcement,
  future-schema preservation, and `heiwa_evidence::read_stream` replay.
- Existing schedule/approval/calendar-sync integration tests — 7/7.
- Cockpit `tsc --noEmit` and Vite production build — green.
- Live Calendar.app: exact writable `Calendar` target, one approved event, one
  external id, one connector receipt event, zero skipped journal lines, exact
  marker/id cleanup, zero verification events left.

## Notes from implementation

Three failures in this session shared one shape — a local check proving less
than it appeared to:

- `cargo clippy` passing was treated as covering static checks; CI runs
  `cargo fmt --all -- --check` as a separate step and it failed.
- The manifest gained `heiwa_vault` while the root lockfile did not. Builds
  locally, fails CI's `--locked`.
- `scripts/ci_rust_test_group.sh` rejected the new crate twice, once for the
  package and once for the integration target. Without that validator the
  crate would have compiled in CI and never had its tests run.

The durable correction is to run the command CI runs, not one assumed to be
equivalent.
