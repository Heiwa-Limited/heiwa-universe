# Google OAuth for the Heiwa Desktop App

> Implementation reference for the L3 connector plane (Calendar and Mail, per AD-14).
> Fetched from Google's identity and Workspace documentation on 2026-08-18.
> Follows the `UPSTREAM_REFERENCES.md` doctrine: distilled reference, not a vendored clone.

## The only viable desktop flow

Two flows Heiwa might have reached for are dead:

- **Out-of-band (OOB)** — the manual copy/paste redirect — is **no longer supported**.
- **Custom URI schemes** are **no longer supported** on desktop, because of app-impersonation risk.

That leaves **loopback IP redirect**, which Google still recommends for macOS, Linux, and
Windows. (Loopback is deprecated for *mobile* only; desktop is unaffected.)

### Loopback mechanics

Bind an HTTP listener on a random available port, then use that port in the redirect URI:

```
http://127.0.0.1:{port}
http://[::1]:{port}
```

The port is chosen at runtime, so the registered redirect URI in Google Cloud Console must
be the loopback form. Heiwa's listener must bind, capture the `code` on the callback, and
shut down immediately — a listener that outlives the exchange is an open local port an
attacker on the machine can talk to.

### PKCE is mandatory

| Field | Requirement |
|---|---|
| `code_verifier` | random, 43–128 chars, alphabet `[A-Z] [a-z] [0-9] - . _ ~` |
| `code_challenge` | BASE64URL(SHA256(verifier)) |
| `code_challenge_method` | `S256` (use this; `plain` is permitted but pointless) |

### Authorization request

`GET https://accounts.google.com/o/oauth2/v2/auth`

Required: `client_id`, `redirect_uri`, `response_type=code`, `scope` (space-delimited),
`code_challenge`, `code_challenge_method`.
Strongly recommended: `state` (CSRF), `login_hint`.

### Token exchange

`POST https://oauth2.googleapis.com/token`

Required: `client_id`, `code`, `code_verifier`, `grant_type=authorization_code`,
`redirect_uri`. `client_secret` is **not** typically used for native apps — which matters,
because a secret shipped inside a distributed desktop binary is not a secret.

Refresh: same endpoint, `grant_type=refresh_token`. Refresh tokens stay valid until the
user revokes access or they expire. Storage goes through `heiwa_vault` / the OS keychain,
never the config root in plaintext.

## Scope classification — the wall

Google sorts scopes into three tiers. The tier, not the API, determines what it costs to
ship to strangers.

### Gmail

| Scope | Tier |
|---|---|
| `gmail.labels` | non-sensitive |
| `gmail.send` | **sensitive** |
| `gmail.readonly` | **restricted** |
| `gmail.metadata` | **restricted** |
| `gmail.compose` | **restricted** |
| `gmail.modify` | **restricted** |
| `mail.google.com` | **restricted** |

### Calendar

Google's Calendar auth page lists ~20 scopes (`calendar`, `calendar.readonly`,
`calendar.events`, `calendar.events.readonly`, `calendar.freebusy`, `calendar.calendarlist`,
ACL and add-on variants) but **does not classify them on that page**. Calendar is not on
the restricted list; treat it as sensitive and confirm the classification at submission
rather than assuming it.

### What each tier costs

- **Non-sensitive** — nothing. Ship it.
- **Sensitive** — brand verification (domain ownership in Search Console, accurate consent
  screen, ~2–3 business days) plus a data-access submission and an unlisted YouTube video
  demonstrating the OAuth flow and each scope in use. Several weeks overall.
- **Restricted** — everything above, **plus an annual third-party security assessment
  (CASA)** by a Google-empanelled assessor, for any app that "has the ability to access
  data from or through a third-party server."

### The exemption that matters right now

Verification is waived for apps that:

- serve only the developer or personal acquaintances,
- are in development/testing,
- access only service-owned data,
- or are internal to a single Google Workspace domain.

**So Devon plus a small circle of testers can run the full Calendar and Gmail flow today
with no verification at all.** The wall appears at public N-user distribution, not at
build time. Build against real scopes now; the submission is a distribution task, not a
prerequisite.

## What this means for Heiwa's design

Reading Gmail through the API is a restricted scope. Reading the user's mail from
**Mail.app on their own machine requires no Google scope, no verification, and no CASA** —
which is what `heiwa mail scan` already does.

That reframes the local-first bridge. It is not a placeholder standing in for "real" cloud
mail; it is the only path to mail reading that a solo publisher can ship to strangers
without an annual paid security assessment. The cloud-API version is the *expensive* one.

The resulting split for L3:

| Capability | Path | Verification cost |
|---|---|---|
| Read mail | Mail.app bridge (built) | none |
| Send mail | `gmail.send` | sensitive |
| Read calendar | Calendar.app bridge (built) + `calendar.readonly` | none / sensitive |
| Write calendar | `calendar.events` | sensitive |

**No restricted scope is required to ship the executive assistant.** Every write goes
through the DREX approval gate and lands a receipt, which is the differentiator the trust
plane exists to serve.

## Sources

- https://developers.google.com/identity/protocols/oauth2/native-app
- https://developers.google.com/workspace/gmail/api/auth/scopes
- https://developers.google.com/workspace/calendar/api/auth
- https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification
