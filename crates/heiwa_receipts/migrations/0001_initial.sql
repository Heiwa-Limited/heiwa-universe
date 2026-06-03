-- heiwa_receipts schema v1
-- See docs/architecture/receipts.md for the canonical spec.

CREATE TABLE IF NOT EXISTS schema_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR IGNORE INTO schema_meta(key, value) VALUES ('schema_version', '1');

CREATE TABLE IF NOT EXISTS receipts (
    id                       TEXT    PRIMARY KEY,
    at                       INTEGER NOT NULL,           -- unix seconds, UTC
    env                      TEXT    NOT NULL CHECK (env IN ('local', 'oauth', 'api')),
    provider                 TEXT    NOT NULL,
    model                    TEXT    NOT NULL,
    agent                    TEXT    NOT NULL,
    tokens_in                INTEGER NOT NULL,
    tokens_out               INTEGER NOT NULL,
    latency_ms               INTEGER NOT NULL,
    actual_cost_cad          REAL    NOT NULL,
    counterfactual_cost_cad  REAL    NOT NULL,
    session_id               TEXT    NOT NULL,
    parent_id                TEXT
);

CREATE INDEX IF NOT EXISTS idx_receipts_at       ON receipts (at);
CREATE INDEX IF NOT EXISTS idx_receipts_session  ON receipts (session_id);
CREATE INDEX IF NOT EXISTS idx_receipts_agent    ON receipts (agent);
CREATE INDEX IF NOT EXISTS idx_receipts_env_at   ON receipts (env, at);
CREATE INDEX IF NOT EXISTS idx_receipts_model_at ON receipts (model, at);
