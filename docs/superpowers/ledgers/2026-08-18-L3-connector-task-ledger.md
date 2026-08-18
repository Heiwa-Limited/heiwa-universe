# L3 Connector Plane — Task Ledger

Contract: `docs/superpowers/specs/2026-08-18-L3-calendar-mail-connectors.md`
Started: 2026-08-18

Status is what is true at HEAD, not what is intended. A row moves to done only
when its verification runs.

## Steps

| # | Step | Status | Verification |
|---|---|---|---|
| 1 | `heiwa_oauth` — loopback + PKCE + exchange | **done** | 27 tests incl. end-to-end flow against a mock provider; `cargo test -p heiwa_oauth` |
| 2 | Token storage shape through `heiwa_vault` | **done** | `session.rs` tests: refresh-token preservation, rotation, saturating expiry |
| 3 | Google Calendar read | blocked | needs an OAuth client id |
| 4 | Calendar write under approval → receipt | blocked | needs 3; this is the L3 acceptance criterion |
| 5 | `gmail.send` on the same path | blocked | needs 3 |

Steps 1 and 2 required no Google account, no client id, and no network. That
was the point of the seam: steps 3–5 are a credential away, not a build away.

## External dependency

Steps 3–5 need a Google Cloud project with an OAuth client of type "Desktop
app", kept in testing mode with Devon's account as a test user. It cannot be
automated — it requires an account, a consent screen, and accepting Google's
terms. Requested 2026-08-18; not yet supplied.

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
