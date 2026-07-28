-- heiwa_receipts schema v3 — executor-side USD accounting truth.
--
-- CAD rate-table columns remain unchanged. These nullable fields preserve the
-- model-call executor's separate USD estimate/report truth and retry spend.

ALTER TABLE receipts ADD COLUMN model_call_cost_usd       REAL;
ALTER TABLE receipts ADD COLUMN model_call_cost_truth     TEXT;
ALTER TABLE receipts ADD COLUMN model_call_attempts       INTEGER;
ALTER TABLE receipts ADD COLUMN failed_attempt_cost_usd   REAL;
