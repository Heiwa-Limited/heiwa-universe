-- heiwa_receipts schema v2 — tamper-evident hash chain.
--
-- Adds an append-only SHA-256 chain over the receipts ledger so any later edit
-- or deletion is detectable. This file only evolves the schema; backfilling the
-- chain over pre-existing rows happens in Rust (`migrate_v2_hash_chain`) because
-- it needs the digest function. Run once, guarded by schema_version in
-- `ReceiptStore::initialise`.

ALTER TABLE receipts ADD COLUMN seq        INTEGER;
ALTER TABLE receipts ADD COLUMN prev_hash  TEXT;
ALTER TABLE receipts ADD COLUMN entry_hash TEXT;

-- seq is the canonical chain order (independent of `at`, which callers supply
-- and may repeat). entry_hash is globally unique because each digest folds in
-- the predecessor's hash. Multiple NULLs are permitted pre-backfill.
CREATE UNIQUE INDEX IF NOT EXISTS idx_receipts_seq        ON receipts (seq);
CREATE UNIQUE INDEX IF NOT EXISTS idx_receipts_entry_hash ON receipts (entry_hash);
