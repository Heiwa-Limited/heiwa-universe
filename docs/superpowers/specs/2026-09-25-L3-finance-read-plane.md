# L3 Finance Read Plane

Date: 2026-09-25
Status: implemented and hermetically verified; live acceptance pending a user key
Plane: Intake + Evidence
Ledger: `docs/superpowers/ledgers/2026-08-18-L3-connector-task-ledger.md` (steps 8–9)

## Summary

Finance joins Calendar and Mail on the L3 connector plane as a read-only lane.
Brokerage accounts arrive through a SnapTrade Personal API key, exchange rates
from the Bank of Canada, and daily prices from Alpha Vantage with the user's own
key. Heiwa stores what it reads as local text truth and derives three read
models from it: the portfolio, registered-account guardrails, and a benchmark
comparison built from the user's real deposits.

Heiwa never places a trade or moves money. That is not a policy switch: no code
path for it exists, a test fails if one is added, and the connector manifests
declare both capabilities `forbidden`.

## Decisions

### AD-37 — SnapTrade Personal is the brokerage lane

Wealthsimple, the first brokerage this was built for, publishes no developer
API. Three routes exist:

| Route | What Heiwa would hold | Licence | Verdict |
|---|---|---|---|
| Unofficial Wealthsimple GraphQL (community client `ws-api-python`) | the brokerage password and a one-time code | GPL-3.0, incompatible with this Apache-2.0 tree | Rejected. It bypasses the platform's published surface, which the autonomy boundary lists as never-do, and puts a brokerage password in Heiwa's hands. |
| SnapTrade Personal API key | a revocable key; the brokerage login happens in SnapTrade's portal | API use | **Chosen.** Free Personal keys, read endpoints for accounts, balances, positions, and transactions, and brokerages beyond Wealthsimple for free. |
| Official statement exports (CSV/PDF) | nothing | n/a | Deferred. The importer must be built against a real export, not a guessed column layout. |

The client is written from SnapTrade's published OpenAPI specification and
verified against signatures computed with the reference SDK's algorithm; no SDK
code is vendored.

### AD-38 — Read-only by construction

The SnapTrade client issues only signed `GET`s against account-data paths.
Account ids are validated before they reach a URL, so an id cannot redirect a
request onto another path. `the_brokerage_client_has_no_write_or_trading_path`
scans the client's production source and fails on any write verb or trading
path. The manifests declare `finance.trade.place` and `finance.funds.move` as
`forbidden` / `unsupported`. Money movement stays with the user, in the
brokerage's own app.

### AD-39 — Read models never touch the credential vault

Keys live in the OS credential vault (`heiwa-connector-keys`). A non-secret
enrollment file under `state/connectors/` records that a lane is connected. The
summary endpoint, which the desktop polls, reads only the enrollment file; only
`heiwa connect` and `heiwa finance sync` read secrets. This keeps a polled
endpoint off the keychain, where a read can block on a macOS consent prompt,
and lets hermetic tests run without touching a real keychain.

Secrets are accepted only at a hidden prompt or on stdin and are refused as
arguments. Each key is verified with one read before it is saved, so a mistyped
key never becomes the stored one.

### AD-40 — Finance reads are authenticated, reads included

Other `GET` read models on the local runtime are unauthenticated. Finance
carries balances and holdings, so every method under `/api/v1/finance/`
requires the same runtime authentication as the operator API. The desktop
proxy already signs every request, so the desktop is unaffected.

### AD-41 — The benchmark comparison withholds a verdict it cannot support

The comparison replays each deposit and withdrawal into the benchmark at the
next close. A flow is priced only when a close exists within six calendar days;
a longer gap means missing history, not a market holiday. With any flow
unpriced the report is `partial` and states no difference, because comparing a
full account value with a partial replay manufactures a result.

## Architecture

| Layer | Path | Role |
|---|---|---|
| Domain crate | `crates/heiwa_finance` | Clients, normalization, store, analytics, sync, summary. No shell or path policy. |
| — `snaptrade` | | Signed read-only client and SnapTrade → Heiwa normalization |
| — `market` | | Bank of Canada Valet FX; Alpha Vantage daily bars, including its HTTP-200 error envelopes |
| — `store` | | Owner-only (0600 in 0700), atomically replaced JSON/JSONL; upsert-by-key merges; exclusive sync lock |
| — `analytics` | | Deterministic read models; callers pass `today` |
| — `sync` | | Source-isolated sync with an Alpha Vantage request budget |
| — `summary` | | `heiwa_finance_summary_v1`, shared by CLI, runtime, and desktop |
| Shell | `apps/heiwa_shell/src/cmd/finance.rs` | CLI, connect flows, vault access, receipts |
| Runtime | `GET /api/v1/finance/summary`, `POST /api/v1/finance/sync` | Authenticated read model and sync |
| Desktop | `apps/heiwa_app/desktop/src/surfaces/finance` | Finance surface; sync is an explicit action |

Local text truth under `<state>/finance/`: `snapshot.json`, `activities.jsonl`,
`bars/<SYMBOL>.jsonl`, `fx/<SERIES>.jsonl`, `settings.json`, `sync.json`.
Account numbers are reduced to their last four characters before storage.

Each sync journals a `heiwa_connector_receipt_v1` event of kind `finance_sync`
to `connector_receipts`. It records counts, lanes, and outcome only: no amounts,
symbols, or account identifiers.

## Read models

**Portfolio.** Values accounts in CAD from the brokerage-reported total, or
holdings plus cash when none is reported. It aggregates positions across
accounts with a weighted average cost and reports currency exposure. Amounts in
a currency without a rate are listed as unconverted rather than guessed.

**TFSA guardrails.**
- Contributions and withdrawals this calendar year, set against the January 1
  room the user copies from CRA My Account. Heiwa cannot know that room itself.
- A note that withdrawals restore room only on the next January 1, and that
  over-contributions are taxed at 1% a month.
- Trading pace: trades in the trailing 365 days and sells within 30 days of a
  buy. Frequent trading can make TFSA income taxable as business income
  (*Ahamed v The King*, 2023 TCC 17, aff'd *Canadian Western Trust Co. v The
  King*, 2024 FCA 108). The level is labelled a Heiwa heuristic because CRA
  applies a facts-and-circumstances test.
- The value of US-listed holdings in a TFSA, where the 15% US withholding tax on
  dividends is not recoverable.

**Benchmark comparison.** The user's own deposits, replayed into a CAD-listed
benchmark (default `XIC.TO`, the S&P/TSX Capped Composite that SPIVA Canada
uses), set against the account's actual value. It answers a user's own question
("would the index have done better with my money?") without predicting
anything.

## Market data constraints (2026-09)

- **Bank of Canada Valet:** official, free, and keyless. It publishes only
  daily rates against CAD. Verified live.
- **Alpha Vantage free tier:** 25 requests a day and about 100 trading days per
  series (`outputsize=full` is premium). TSX symbols use `.TRT`. Each sync
  spends at most 8 requests and skips series fetched in the last 20 hours;
  history accumulates locally across syncs.
- **Excluded:** Stooq now requires a CAPTCHA-issued key. Twelve Data's free tier
  excludes the TSX. Alpaca and Massive (formerly Polygon) cover US markets only.
  Yahoo endpoints are undocumented.

## Research roadmap

A design review of an externally proposed "quant" plan held its core claims
against primary sources:
- Most active Canadian equity funds trail their benchmark over ten years
  (SPIVA Canada).
- Factor premia and momentum are documented (Fama–French 2015;
  Jegadeesh–Titman 1993; Hurst–Ooi–Pedersen's century of trend-following).
- Machine learning extracts weak but real cross-sectional signal (Gu–Kelly–Xiu,
  *Review of Financial Studies* 2020).
- Unconstrained backtest search manufactures false discoveries (Bailey,
  Borwein, López de Prado, Zhu 2014; the deflated Sharpe ratio).

The review also found one point that disqualifies part of the plan for Canadian
users. A tactical sleeve trading over hours or days inside a TFSA risks the
business-income treatment above. Heiwa therefore sequences the work as:

| Phase | Deliverable | Gate |
|---|---|---|
| R0 (this spec) | Read plane plus benchmark comparison: the measuring stick | Live acceptance with a user key |
| R1 | Deeper point-in-time price history, time- and money-weighted returns, drawdowns | A price source with full history |
| R2 | Paper ledger and signal-research harness: walk-forward and purged validation, realistic costs (spread plus FX conversion), deflated Sharpe, multiple-testing correction | Out-of-sample evidence recorded as receipts |
| R3 | Research shortlists surfaced as read-only notes | Never an allocation or order instruction |
| Never | Heiwa placing orders or moving money | — |

`apps/heiwa_trading` already runs a paper-trading harness and is the natural
home for R2.

## Verification (2026-09-25)

- `cargo test -p heiwa_finance`: 55 passed.
- `cargo test -p heiwa_finance -- --ignored live_`: live Valet read passed.
- `cargo test -p heiwa-shell --test finance_cli`: 7 passed.
- Shell unit tests covering finance auth, connectors, and the CLI helpers pass.
- Desktop `vitest run`: 201 passed. `tsc --noEmit` is clean.
- `python3 scripts/validate_connector_manifests.py`: ok.
- Checkout runtime on port 7475 with a disposable home:
  - Unauthenticated `GET /api/v1/finance/summary` returned 401.
  - An authenticated request returned `heiwa_finance_summary_v1`.
  - `POST /api/v1/finance/sync` reported the unconnected state without network
    access.

## Open items

- Live acceptance: one `heiwa finance sync` against a real Personal key with a
  linked brokerage, and one with an Alpha Vantage key. Then move the manifests'
  read capabilities to `live`.
- Statement-export import, built against a real export.
- Feeding the portfolio into the life read model's `money` domain.
- Price history beyond the free tier's window.
- After a binary update, macOS may ask once before the runtime reads a key
  saved by an earlier build. The OAuth tokens behave the same way.
