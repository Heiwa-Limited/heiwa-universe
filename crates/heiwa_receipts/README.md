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
- `Receipt::header()` — STDB-safe subset

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

Counterfactual is per-rate-entry — an OAuth lane's counterfactual is its API
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

let now_unix = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)?
    .as_secs() as i64;

let r = Receipt::new(
    now_unix,
    Env::Api,
    "openrouter",
    "claude-3.7-sonnet",
    "trading",
    9_000,
    3_000,
    72,
    costs.actual_cad,
    costs.counterfactual_cad,
    "sess-...",
    None,
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
