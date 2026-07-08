# heiwa_receipts

Local SQLite receipt store. One row per cost-bearing call. The schema and
operator-facing rollups described in [`docs/architecture/receipts.md`](../../docs/architecture/receipts.md).

## Status

Stub. What is implemented today:

- v1 SQLite schema (`migrations/0001_initial.sql`)
- `ReceiptStore::open` / `open_in_memory`
- Synchronous `insert`, `get`, `list`
- `rollup_by_env` / `rollup_by_agent` / `rollup_by_model`
- `day_total` for the hero readout
- `RateTable::from_toml_str` + `compute(env, provider, model, tokens_in, tokens_out)`
- `Receipt::header()` — STDB-safe subset (currently no prompt fields exist
  in the row, but the helper is in place so future fields cannot leak)

What is **not** implemented:

- STDB mirror (the header helper exists; the network path does not)
- Prompt-body storage (`~/.heiwa/prompts/<id>.txt`)
- Write-ahead log catch-up after crash
- CLI surface (`heiwa cost`, `heiwa receipts`, `heiwa headroom`)
- Migration framework beyond the v1 initial
- Latency percentile queries (SQLite needs an extension or table scan)

## Cost calculation

Two cost columns per receipt:

- `actual_cost_cad` — what the operator actually paid for this call.
- `counterfactual_cost_cad` — what the same tokens would have cost on the
  metered API lane for this model.

Rates are loaded from `~/.heiwa/rates.toml` (see [`docs/architecture/receipts.md#rate-table-example`](../../docs/architecture/receipts.md)).
Counterfactual is per-entry — an OAuth lane's counterfactual is its API
equivalent; a local lane's counterfactual is the nearest-equivalent hosted
model. The runtime stores the chosen number at receipt-write time so reads
do not depend on the rate-table version at query time.

## Usage

```rust
use heiwa_receipts::{Env, RateTable, Receipt, ReceiptStore};

let store = ReceiptStore::open("~/.heiwa/receipts.db")?;
let rates = RateTable::from_path("~/.heiwa/rates.toml")?;

let costs = rates.compute(
    Env::Api, "openrouter", "claude-3.7-sonnet",
    9_000, 3_000,
)?;

let r = Receipt::new(
    /* at        */ chrono::Utc::now().timestamp(),
    /* env       */ Env::Api,
    /* provider  */ "openrouter",
    /* model     */ "claude-3.7-sonnet",
    /* agent     */ "trading",
    /* tokens_in */ 9_000,
    /* tokens_out*/ 3_000,
    /* latency   */ 72,
    /* actual    */ costs.actual_cad,
    /* counterf. */ costs.counterfactual_cad,
    /* session   */ "sess-...",
    /* parent_id */ None,
);
store.insert(&r)?;
```

## Testing

```bash
cargo test -p heiwa_receipts
```

`tests/smoke.rs` mirrors the marketing-site cost-attribution ledger end to end
(seed five receipts, verify rollups, verify totals, verify header redaction).

## Privacy boundary

The store has full content. `Receipt::header()` is the only public path that
should ever feed STDB; the type is deliberately distinct from `Receipt` so a
caller cannot accidentally pass the wrong struct across the network boundary.
