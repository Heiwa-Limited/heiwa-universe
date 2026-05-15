//! Local SQLite quota ledger for Heiwa.
//!
//! Tracks per-provider, per-rate-group usage windows plus a run history for
//! replay and cost attribution. All state lives under `~/.heiwa/state.db` by
//! default (configurable). Replaces the STDB cross-device ledger — on-machine
//! only until VPS phase.
//!
//! Schema:
//! - `quota_state(provider, rate_group, window_start_unix, window_seconds,
//!    tokens_used, requests, updated_at_unix)`
//! - `run_history(id, provider, model_id, started_at_unix, ended_at_unix,
//!    tokens_input, tokens_output, cost, status, meta_json)`
//!
//! Designed for single-process access. Heiwa runtime is per-user. For
//! multi-process coordination rely on SQLite's file lock.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum QuotaError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid schema version: found {found}, expected {expected}")]
    SchemaVersion { found: i64, expected: i64 },
}

pub type Result<T> = std::result::Result<T, QuotaError>;

const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaState {
    pub provider: String,
    pub rate_group: String,
    pub window_start_unix: i64,
    pub window_seconds: i64,
    pub tokens_used: i64,
    pub requests: i64,
    pub updated_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemainingBudget {
    pub provider: String,
    pub rate_group: String,
    pub window_seconds: i64,
    pub token_limit: i64,
    pub tokens_used: i64,
    pub tokens_remaining: i64,
    pub requests: i64,
    pub window_resets_at_unix: i64,
    pub exhausted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunRecord {
    pub id: String,
    pub provider: String,
    pub model_id: String,
    pub started_at_unix: i64,
    pub ended_at_unix: i64,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub cost: f64,
    pub status: String,
    pub meta: serde_json::Value,
}

pub struct QuotaLedger {
    conn: Mutex<Connection>,
}

impl QuotaLedger {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        let ledger = Self {
            conn: Mutex::new(conn),
        };
        ledger.migrate()?;
        Ok(ledger)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let ledger = Self {
            conn: Mutex::new(conn),
        };
        ledger.migrate()?;
        Ok(ledger)
    }

    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("HEIWA_STATE_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs_home().map(|h| h.join(".heiwa")))
            .unwrap_or_else(|| PathBuf::from(".heiwa"));
        base.join("state.db")
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().expect("lock");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS schema_info (
                key TEXT PRIMARY KEY,
                value INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS quota_state (
                provider TEXT NOT NULL,
                rate_group TEXT NOT NULL,
                window_start_unix INTEGER NOT NULL,
                window_seconds INTEGER NOT NULL,
                tokens_used INTEGER NOT NULL DEFAULT 0,
                requests INTEGER NOT NULL DEFAULT 0,
                updated_at_unix INTEGER NOT NULL,
                PRIMARY KEY (provider, rate_group)
            );
            CREATE TABLE IF NOT EXISTS run_history (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                model_id TEXT NOT NULL,
                started_at_unix INTEGER NOT NULL,
                ended_at_unix INTEGER NOT NULL,
                tokens_input INTEGER NOT NULL DEFAULT 0,
                tokens_output INTEGER NOT NULL DEFAULT 0,
                cost REAL NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                meta_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_run_history_started
                ON run_history (started_at_unix DESC);
            CREATE INDEX IF NOT EXISTS idx_run_history_provider
                ON run_history (provider, started_at_unix DESC);
            ",
        )?;
        let current: Option<i64> = conn
            .query_row(
                "SELECT value FROM schema_info WHERE key = 'version'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        match current {
            None => {
                conn.execute(
                    "INSERT INTO schema_info (key, value) VALUES ('version', ?1)",
                    params![SCHEMA_VERSION],
                )?;
            }
            Some(v) if v == SCHEMA_VERSION => {}
            Some(v) => {
                return Err(QuotaError::SchemaVersion {
                    found: v,
                    expected: SCHEMA_VERSION,
                });
            }
        }
        Ok(())
    }

    pub fn upsert_quota(&self, state: &QuotaState) -> Result<()> {
        let conn = self.conn.lock().expect("lock");
        conn.execute(
            "INSERT INTO quota_state
                (provider, rate_group, window_start_unix, window_seconds,
                 tokens_used, requests, updated_at_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(provider, rate_group) DO UPDATE SET
                window_start_unix = excluded.window_start_unix,
                window_seconds = excluded.window_seconds,
                tokens_used = excluded.tokens_used,
                requests = excluded.requests,
                updated_at_unix = excluded.updated_at_unix",
            params![
                state.provider,
                state.rate_group,
                state.window_start_unix,
                state.window_seconds,
                state.tokens_used,
                state.requests,
                state.updated_at_unix,
            ],
        )?;
        Ok(())
    }

    /// Increment counters for the current window. If the recorded window has
    /// expired (`now >= window_start + window_seconds`), the window is reset
    /// with `now` as the new start.
    pub fn record_use(
        &self,
        provider: &str,
        rate_group: &str,
        window_seconds: i64,
        tokens: i64,
        requests: i64,
        now_unix: i64,
    ) -> Result<QuotaState> {
        let conn = self.conn.lock().expect("lock");
        let existing: Option<(i64, i64, i64, i64)> = conn
            .query_row(
                "SELECT window_start_unix, window_seconds, tokens_used, requests
                 FROM quota_state
                 WHERE provider = ?1 AND rate_group = ?2",
                params![provider, rate_group],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;

        let (window_start, tokens_used, reqs) = match existing {
            Some((start, secs, used, r)) if now_unix < start.saturating_add(secs) => {
                (start, used + tokens, r + requests)
            }
            _ => (now_unix, tokens, requests),
        };

        conn.execute(
            "INSERT INTO quota_state
                (provider, rate_group, window_start_unix, window_seconds,
                 tokens_used, requests, updated_at_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(provider, rate_group) DO UPDATE SET
                window_start_unix = excluded.window_start_unix,
                window_seconds = excluded.window_seconds,
                tokens_used = excluded.tokens_used,
                requests = excluded.requests,
                updated_at_unix = excluded.updated_at_unix",
            params![
                provider,
                rate_group,
                window_start,
                window_seconds,
                tokens_used,
                reqs,
                now_unix,
            ],
        )?;

        Ok(QuotaState {
            provider: provider.to_string(),
            rate_group: rate_group.to_string(),
            window_start_unix: window_start,
            window_seconds,
            tokens_used,
            requests: reqs,
            updated_at_unix: now_unix,
        })
    }

    pub fn get_quota(&self, provider: &str, rate_group: &str) -> Result<Option<QuotaState>> {
        let conn = self.conn.lock().expect("lock");
        let state = conn
            .query_row(
                "SELECT provider, rate_group, window_start_unix, window_seconds,
                        tokens_used, requests, updated_at_unix
                 FROM quota_state
                 WHERE provider = ?1 AND rate_group = ?2",
                params![provider, rate_group],
                |row| {
                    Ok(QuotaState {
                        provider: row.get(0)?,
                        rate_group: row.get(1)?,
                        window_start_unix: row.get(2)?,
                        window_seconds: row.get(3)?,
                        tokens_used: row.get(4)?,
                        requests: row.get(5)?,
                        updated_at_unix: row.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(state)
    }

    pub fn remaining_budget(
        &self,
        provider: &str,
        rate_group: &str,
        window_seconds: i64,
        token_limit: i64,
        now_unix: i64,
    ) -> Result<RemainingBudget> {
        let existing = self.get_quota(provider, rate_group)?;
        let (tokens_used, requests, window_resets_at_unix) = match existing {
            Some(state)
                if now_unix < state.window_start_unix.saturating_add(state.window_seconds) =>
            {
                (
                    state.tokens_used.max(0),
                    state.requests.max(0),
                    state.window_start_unix.saturating_add(state.window_seconds),
                )
            }
            _ => (0, 0, now_unix.saturating_add(window_seconds)),
        };

        let tokens_remaining = if token_limit <= 0 {
            0
        } else {
            token_limit.saturating_sub(tokens_used).max(0)
        };

        Ok(RemainingBudget {
            provider: provider.to_string(),
            rate_group: rate_group.to_string(),
            window_seconds,
            token_limit,
            tokens_used,
            tokens_remaining,
            requests,
            window_resets_at_unix,
            exhausted: token_limit > 0 && tokens_remaining == 0,
        })
    }

    pub fn list_quotas(&self) -> Result<Vec<QuotaState>> {
        let conn = self.conn.lock().expect("lock");
        let mut stmt = conn.prepare(
            "SELECT provider, rate_group, window_start_unix, window_seconds,
                    tokens_used, requests, updated_at_unix
             FROM quota_state
             ORDER BY provider, rate_group",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(QuotaState {
                provider: row.get(0)?,
                rate_group: row.get(1)?,
                window_start_unix: row.get(2)?,
                window_seconds: row.get(3)?,
                tokens_used: row.get(4)?,
                requests: row.get(5)?,
                updated_at_unix: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn record_run(&self, run: &RunRecord) -> Result<()> {
        let conn = self.conn.lock().expect("lock");
        let meta = serde_json::to_string(&run.meta)?;
        conn.execute(
            "INSERT OR REPLACE INTO run_history
                (id, provider, model_id, started_at_unix, ended_at_unix,
                 tokens_input, tokens_output, cost, status, meta_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                run.id,
                run.provider,
                run.model_id,
                run.started_at_unix,
                run.ended_at_unix,
                run.tokens_input,
                run.tokens_output,
                run.cost,
                run.status,
                meta,
            ],
        )?;
        Ok(())
    }

    pub fn recent_runs(&self, limit: i64) -> Result<Vec<RunRecord>> {
        let conn = self.conn.lock().expect("lock");
        let mut stmt = conn.prepare(
            "SELECT id, provider, model_id, started_at_unix, ended_at_unix,
                    tokens_input, tokens_output, cost, status, meta_json
             FROM run_history
             ORDER BY started_at_unix DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            let meta_json: String = row.get(9)?;
            let meta: serde_json::Value =
                serde_json::from_str(&meta_json).unwrap_or(serde_json::Value::Null);
            Ok(RunRecord {
                id: row.get(0)?,
                provider: row.get(1)?,
                model_id: row.get(2)?,
                started_at_unix: row.get(3)?,
                ended_at_unix: row.get(4)?,
                tokens_input: row.get(5)?,
                tokens_output: row.get(6)?,
                cost: row.get(7)?,
                status: row.get(8)?,
                meta,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

fn dirs_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger() -> QuotaLedger {
        QuotaLedger::open_in_memory().expect("open in-memory ledger")
    }

    #[test]
    fn schema_bootstraps() {
        let l = ledger();
        assert!(l.list_quotas().unwrap().is_empty());
        assert!(l.recent_runs(10).unwrap().is_empty());
    }

    #[test]
    fn record_use_creates_new_window() {
        let l = ledger();
        let state = l
            .record_use("claude-code", "anthropic", 60, 100, 1, 1_000)
            .unwrap();
        assert_eq!(state.window_start_unix, 1_000);
        assert_eq!(state.tokens_used, 100);
        assert_eq!(state.requests, 1);
    }

    #[test]
    fn record_use_accumulates_within_window() {
        let l = ledger();
        l.record_use("claude-code", "anthropic", 60, 100, 1, 1_000)
            .unwrap();
        let state = l
            .record_use("claude-code", "anthropic", 60, 50, 1, 1_010)
            .unwrap();
        assert_eq!(state.window_start_unix, 1_000);
        assert_eq!(state.tokens_used, 150);
        assert_eq!(state.requests, 2);
    }

    #[test]
    fn record_use_resets_after_window_expires() {
        let l = ledger();
        l.record_use("claude-code", "anthropic", 60, 100, 1, 1_000)
            .unwrap();
        let state = l
            .record_use("claude-code", "anthropic", 60, 25, 1, 1_200)
            .unwrap();
        assert_eq!(state.window_start_unix, 1_200);
        assert_eq!(state.tokens_used, 25);
        assert_eq!(state.requests, 1);
    }

    #[test]
    fn remaining_budget_marks_active_window_exhausted() {
        let l = ledger();
        l.record_use("claude-code", "anthropic", 60, 100, 2, 1_000)
            .unwrap();

        let budget = l
            .remaining_budget("claude-code", "anthropic", 60, 100, 1_010)
            .unwrap();

        assert_eq!(budget.tokens_used, 100);
        assert_eq!(budget.tokens_remaining, 0);
        assert_eq!(budget.requests, 2);
        assert_eq!(budget.window_resets_at_unix, 1_060);
        assert!(budget.exhausted);
    }

    #[test]
    fn remaining_budget_resets_after_window_expires() {
        let l = ledger();
        l.record_use("claude-code", "anthropic", 60, 100, 1, 1_000)
            .unwrap();

        let budget = l
            .remaining_budget("claude-code", "anthropic", 60, 100, 1_200)
            .unwrap();

        assert_eq!(budget.tokens_used, 0);
        assert_eq!(budget.tokens_remaining, 100);
        assert_eq!(budget.requests, 0);
        assert_eq!(budget.window_resets_at_unix, 1_260);
        assert!(!budget.exhausted);
    }

    #[test]
    fn get_and_list_quotas() {
        let l = ledger();
        l.record_use("claude-code", "anthropic", 60, 100, 1, 1_000)
            .unwrap();
        l.record_use("ollama", "local", 60, 0, 1, 1_000).unwrap();
        assert_eq!(l.list_quotas().unwrap().len(), 2);
        let a = l.get_quota("claude-code", "anthropic").unwrap().unwrap();
        assert_eq!(a.tokens_used, 100);
        assert!(l.get_quota("missing", "none").unwrap().is_none());
    }

    #[test]
    fn run_history_round_trip() {
        let l = ledger();
        let run = RunRecord {
            id: "run-1".into(),
            provider: "claude-code".into(),
            model_id: "claude-opus-4-7".into(),
            started_at_unix: 100,
            ended_at_unix: 105,
            tokens_input: 1_000,
            tokens_output: 500,
            cost: 0.0,
            status: "completed".into(),
            meta: serde_json::json!({ "session": "abc" }),
        };
        l.record_run(&run).unwrap();
        let runs = l.recent_runs(10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0], run);
    }

    #[test]
    fn recent_runs_orders_descending() {
        let l = ledger();
        for i in 0..3 {
            l.record_run(&RunRecord {
                id: format!("r-{i}"),
                provider: "p".into(),
                model_id: "m".into(),
                started_at_unix: i,
                ended_at_unix: i + 1,
                tokens_input: 0,
                tokens_output: 0,
                cost: 0.0,
                status: "ok".into(),
                meta: serde_json::Value::Null,
            })
            .unwrap();
        }
        let runs = l.recent_runs(10).unwrap();
        assert_eq!(
            runs.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["r-2", "r-1", "r-0"]
        );
    }

    #[test]
    fn persists_across_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.db");
        {
            let l = QuotaLedger::open(&path).unwrap();
            l.record_use("p", "g", 60, 10, 1, 1_000).unwrap();
        }
        let l = QuotaLedger::open(&path).unwrap();
        let state = l.get_quota("p", "g").unwrap().unwrap();
        assert_eq!(state.tokens_used, 10);
    }
}
