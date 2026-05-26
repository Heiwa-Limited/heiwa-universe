//! Local SQLite receipt store for Heiwa.
//!
//! One row per cost-bearing call. Every operator view — by lane, by agent, by
//! model, by day — is a `SUM(...) GROUP BY ...` rollup over this table. CAD is
//! the storage base unit; presentation in other currencies is a divide-at-read
//! overlay handled at the UI layer.
//!
//! See `docs/architecture/receipts.md` for the canonical spec.
//!
//! ## Status (stub)
//!
//! - Schema, insert, query, env/agent/model rollups: implemented.
//! - Rate-table loading + cost computation (actual + counterfactual): implemented.
//! - STDB header mirror: **not implemented** — `header()` returns the redactable
//!   subset for whatever layer wires the mirror.
//! - Prompt bodies, WAL catch-up, CLI surface: **not implemented**.
//! - `id` is currently `uuid v4`; spec calls for ULID. Ordering is by `at`, not
//!   by id, so switching id type later requires no schema migration.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 1;
const INITIAL_SQL: &str = include_str!("../migrations/0001_initial.sql");

#[derive(Debug, Error)]
pub enum ReceiptError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid env: {0}")]
    InvalidEnv(String),
    #[error("rate not found: env={env:?} provider={provider} model={model}")]
    RateNotFound {
        env: Env,
        provider: String,
        model: String,
    },
    #[error("invalid schema version: found {found}, expected {expected}")]
    SchemaVersion { found: i64, expected: i64 },
    #[error("store lock poisoned")]
    LockPoisoned,
}

pub type Result<T> = std::result::Result<T, ReceiptError>;

// ============================================================================
// Domain types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Env {
    Local,
    Oauth,
    Api,
}

impl Env {
    pub fn as_str(&self) -> &'static str {
        match self {
            Env::Local => "local",
            Env::Oauth => "oauth",
            Env::Api => "api",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "local" => Ok(Env::Local),
            "oauth" => Ok(Env::Oauth),
            "api" => Ok(Env::Api),
            other => Err(ReceiptError::InvalidEnv(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Receipt {
    pub id: String,
    pub at: i64,
    pub env: Env,
    pub provider: String,
    pub model: String,
    pub agent: String,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub latency_ms: i64,
    pub actual_cost_cad: f64,
    pub counterfactual_cost_cad: f64,
    pub session_id: String,
    pub parent_id: Option<String>,
}

impl Receipt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        at: i64,
        env: Env,
        provider: impl Into<String>,
        model: impl Into<String>,
        agent: impl Into<String>,
        tokens_in: i64,
        tokens_out: i64,
        latency_ms: i64,
        actual_cost_cad: f64,
        counterfactual_cost_cad: f64,
        session_id: impl Into<String>,
        parent_id: Option<String>,
    ) -> Self {
        Receipt {
            id: Uuid::new_v4().to_string(),
            at,
            env,
            provider: provider.into(),
            model: model.into(),
            agent: agent.into(),
            tokens_in,
            tokens_out,
            latency_ms,
            actual_cost_cad,
            counterfactual_cost_cad,
            session_id: session_id.into(),
            parent_id,
        }
    }

    pub fn header(&self) -> ReceiptHeader {
        ReceiptHeader {
            id: self.id.clone(),
            at: self.at,
            env: self.env,
            provider: self.provider.clone(),
            model: self.model.clone(),
            agent: Some(self.agent.clone()),
            tokens_in: self.tokens_in,
            tokens_out: self.tokens_out,
            latency_ms: self.latency_ms,
            actual_cost_cad: self.actual_cost_cad,
            counterfactual_cost_cad: self.counterfactual_cost_cad,
            schema_version: SCHEMA_VERSION,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReceiptHeader {
    pub id: String,
    pub at: i64,
    pub env: Env,
    pub provider: String,
    pub model: String,
    pub agent: Option<String>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub latency_ms: i64,
    pub actual_cost_cad: f64,
    pub counterfactual_cost_cad: f64,
    pub schema_version: i64,
}

// ============================================================================
// Rate table
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateEntry {
    pub input_per_mtok_cad: f64,
    pub output_per_mtok_cad: f64,
    #[serde(default)]
    pub counterfactual: Option<CounterfactualRate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CounterfactualRate {
    pub input_per_mtok_cad: f64,
    pub output_per_mtok_cad: f64,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Costs {
    pub actual_cad: f64,
    pub counterfactual_cad: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RateTable {
    #[serde(default)]
    pub synced_at: Option<String>,
    #[serde(default)]
    pub rates: RateMap,
}

type RateMap = HashMap<String, HashMap<String, HashMap<String, RateEntry>>>;

impl RateTable {
    pub fn from_toml_str(s: &str) -> Result<Self> {
        Ok(toml::from_str(s)?)
    }

    pub fn from_path(p: impl AsRef<Path>) -> Result<Self> {
        let bytes = std::fs::read_to_string(p)?;
        Self::from_toml_str(&bytes)
    }

    pub fn compute(
        &self,
        env: Env,
        provider: &str,
        model: &str,
        tokens_in: i64,
        tokens_out: i64,
    ) -> Result<Costs> {
        let entry = self
            .rates
            .get(env.as_str())
            .and_then(|provs| provs.get(provider))
            .and_then(|mods| mods.get(model))
            .ok_or_else(|| ReceiptError::RateNotFound {
                env,
                provider: provider.to_string(),
                model: model.to_string(),
            })?;

        let m_in = tokens_in as f64 / 1_000_000.0;
        let m_out = tokens_out as f64 / 1_000_000.0;

        let actual = m_in * entry.input_per_mtok_cad + m_out * entry.output_per_mtok_cad;

        let counterfactual = match &entry.counterfactual {
            Some(cf) => m_in * cf.input_per_mtok_cad + m_out * cf.output_per_mtok_cad,
            None => actual,
        };

        Ok(Costs {
            actual_cad: actual,
            counterfactual_cad: counterfactual,
        })
    }
}

// ============================================================================
// Rollups
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnvRollup {
    pub env: Env,
    pub calls: i64,
    pub tokens: i64,
    pub actual_cost_cad: f64,
    pub counterfactual_cost_cad: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRollup {
    pub agent: String,
    pub calls: i64,
    pub tokens: i64,
    pub actual_cost_cad: f64,
    pub counterfactual_cost_cad: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelRollup {
    pub provider: String,
    pub model: String,
    pub calls: i64,
    pub tokens: i64,
    pub actual_cost_cad: f64,
    pub counterfactual_cost_cad: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct DayTotal {
    pub tokens: i64,
    pub actual_cost_cad: f64,
    pub counterfactual_cost_cad: f64,
}

// ============================================================================
// Store
// ============================================================================

pub struct ReceiptStore {
    conn: Mutex<Connection>,
}

impl ReceiptStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        Self::initialise(&conn)?;
        Ok(ReceiptStore {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::initialise(&conn)?;
        Ok(ReceiptStore {
            conn: Mutex::new(conn),
        })
    }

    fn initialise(conn: &Connection) -> Result<()> {
        conn.execute_batch(INITIAL_SQL)?;
        let found: i64 = conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM schema_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(SCHEMA_VERSION);
        if found != SCHEMA_VERSION {
            return Err(ReceiptError::SchemaVersion {
                found,
                expected: SCHEMA_VERSION,
            });
        }
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| ReceiptError::LockPoisoned)
    }

    pub fn insert(&self, r: &Receipt) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO receipts (
                id, at, env, provider, model, agent,
                tokens_in, tokens_out, latency_ms,
                actual_cost_cad, counterfactual_cost_cad,
                session_id, parent_id
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9,
                ?10, ?11,
                ?12, ?13
            )",
            params![
                r.id,
                r.at,
                r.env.as_str(),
                r.provider,
                r.model,
                r.agent,
                r.tokens_in,
                r.tokens_out,
                r.latency_ms,
                r.actual_cost_cad,
                r.counterfactual_cost_cad,
                r.session_id,
                r.parent_id,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<Receipt>> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT * FROM receipts WHERE id = ?1",
                params![id],
                row_to_receipt,
            )
            .optional()?;
        Ok(row)
    }

    pub fn list(&self, since_unix: i64, until_unix: i64) -> Result<Vec<Receipt>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, at, env, provider, model, agent,
                    tokens_in, tokens_out, latency_ms,
                    actual_cost_cad, counterfactual_cost_cad,
                    session_id, parent_id
             FROM receipts
             WHERE at >= ?1 AND at < ?2
             ORDER BY at DESC",
        )?;
        let rows = stmt
            .query_map(params![since_unix, until_unix], row_to_receipt)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn rollup_by_env(&self, since_unix: i64) -> Result<Vec<EnvRollup>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT env,
                    COUNT(*)                              AS calls,
                    SUM(tokens_in + tokens_out)           AS tokens,
                    COALESCE(SUM(actual_cost_cad),0)      AS actual,
                    COALESCE(SUM(counterfactual_cost_cad),0) AS counterfactual
             FROM receipts
             WHERE at >= ?1
             GROUP BY env
             ORDER BY tokens DESC",
        )?;
        let rows = stmt
            .query_map(params![since_unix], |row| {
                let env: String = row.get(0)?;
                Ok((
                    env,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut out = Vec::with_capacity(rows.len());
        for (env, calls, tokens, actual, counterfactual) in rows {
            out.push(EnvRollup {
                env: Env::parse(&env)?,
                calls,
                tokens,
                actual_cost_cad: actual,
                counterfactual_cost_cad: counterfactual,
            });
        }
        Ok(out)
    }

    pub fn rollup_by_agent(&self, since_unix: i64) -> Result<Vec<AgentRollup>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT agent,
                    COUNT(*),
                    SUM(tokens_in + tokens_out),
                    COALESCE(SUM(actual_cost_cad),0),
                    COALESCE(SUM(counterfactual_cost_cad),0)
             FROM receipts
             WHERE at >= ?1
             GROUP BY agent
             ORDER BY 3 DESC",
        )?;
        let rows = stmt
            .query_map(params![since_unix], |row| {
                Ok(AgentRollup {
                    agent: row.get(0)?,
                    calls: row.get(1)?,
                    tokens: row.get(2)?,
                    actual_cost_cad: row.get(3)?,
                    counterfactual_cost_cad: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn rollup_by_model(&self, since_unix: i64) -> Result<Vec<ModelRollup>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT provider, model,
                    COUNT(*),
                    SUM(tokens_in + tokens_out),
                    COALESCE(SUM(actual_cost_cad),0),
                    COALESCE(SUM(counterfactual_cost_cad),0)
             FROM receipts
             WHERE at >= ?1
             GROUP BY provider, model
             ORDER BY 4 DESC",
        )?;
        let rows = stmt
            .query_map(params![since_unix], |row| {
                Ok(ModelRollup {
                    provider: row.get(0)?,
                    model: row.get(1)?,
                    calls: row.get(2)?,
                    tokens: row.get(3)?,
                    actual_cost_cad: row.get(4)?,
                    counterfactual_cost_cad: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn day_total(&self, since_unix: i64) -> Result<DayTotal> {
        let conn = self.lock()?;
        let row = conn.query_row(
            "SELECT COALESCE(SUM(tokens_in + tokens_out),0),
                    COALESCE(SUM(actual_cost_cad),0),
                    COALESCE(SUM(counterfactual_cost_cad),0)
             FROM receipts
             WHERE at >= ?1",
            params![since_unix],
            |row| {
                Ok(DayTotal {
                    tokens: row.get(0)?,
                    actual_cost_cad: row.get(1)?,
                    counterfactual_cost_cad: row.get(2)?,
                })
            },
        )?;
        Ok(row)
    }

    pub fn schema_version(&self) -> Result<i64> {
        let conn = self.lock()?;
        let v: i64 = conn.query_row(
            "SELECT CAST(value AS INTEGER) FROM schema_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )?;
        Ok(v)
    }
}

fn row_to_receipt(row: &rusqlite::Row<'_>) -> rusqlite::Result<Receipt> {
    let env: String = row.get("env")?;
    Ok(Receipt {
        id: row.get("id")?,
        at: row.get("at")?,
        env: match env.as_str() {
            "local" => Env::Local,
            "oauth" => Env::Oauth,
            "api" => Env::Api,
            _ => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, env)),
                ))
            }
        },
        provider: row.get("provider")?,
        model: row.get("model")?,
        agent: row.get("agent")?,
        tokens_in: row.get("tokens_in")?,
        tokens_out: row.get("tokens_out")?,
        latency_ms: row.get("latency_ms")?,
        actual_cost_cad: row.get("actual_cost_cad")?,
        counterfactual_cost_cad: row.get("counterfactual_cost_cad")?,
        session_id: row.get("session_id")?,
        parent_id: row.get("parent_id")?,
    })
}

// ============================================================================
// Runtime helpers — what callers at the shell/runtime boundary use
// ============================================================================

/// Convenience module so callers don't reinvent the rates loader + env mapping.
pub mod runtime {
    use super::*;

    /// Built-in fallback rates matching the marketing-demo conventions on
    /// heiwa.ltd. Operators override by writing `~/.heiwa/rates.toml`.
    pub fn default_rates() -> RateTable {
        const DEFAULT_TOML: &str = r#"
synced_at = "2026-05-25T00:00:00Z"

[rates.local.ollama."qwen3.5:9b"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
[rates.local.ollama."qwen3.5:9b".counterfactual]
input_per_mtok_cad  = 0.27
output_per_mtok_cad = 0.81
note = "Mistral 7B tier as fairness proxy"

[rates.local.ollama."qwen3.5:4b"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
[rates.local.ollama."qwen3.5:4b".counterfactual]
input_per_mtok_cad  = 0.14
output_per_mtok_cad = 0.42

[rates.local.ollama."gemma4"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
[rates.local.ollama."gemma4".counterfactual]
input_per_mtok_cad  = 0.27
output_per_mtok_cad = 0.81

[rates.oauth."claude-code"."claude-sonnet-4-6"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
[rates.oauth."claude-code"."claude-sonnet-4-6".counterfactual]
input_per_mtok_cad  = 4.05
output_per_mtok_cad = 20.25

[rates.oauth."claude-code"."claude-opus-4-7"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
[rates.oauth."claude-code"."claude-opus-4-7".counterfactual]
input_per_mtok_cad  = 20.25
output_per_mtok_cad = 101.25

[rates.oauth.codex."gpt-5-codex"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
[rates.oauth.codex."gpt-5-codex".counterfactual]
input_per_mtok_cad  = 2.75
output_per_mtok_cad = 11.00

[rates.oauth.gemini."gemini-3.1-pro"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
[rates.oauth.gemini."gemini-3.1-pro".counterfactual]
input_per_mtok_cad  = 1.69
output_per_mtok_cad = 6.75

[rates.api.openrouter."claude-3.7-sonnet"]
input_per_mtok_cad  = 4.05
output_per_mtok_cad = 20.25
"#;
        RateTable::from_toml_str(DEFAULT_TOML).expect("default rates parse")
    }

    /// Read `~/.heiwa/rates.toml` if present; otherwise return defaults.
    /// Parse failures silently fall back too — corrupt rate files should not
    /// stop the runtime from writing receipts.
    pub fn load_rates_or_default(heiwa_home: &std::path::Path) -> RateTable {
        let path = heiwa_home.join("rates.toml");
        if !path.exists() {
            return default_rates();
        }
        match RateTable::from_path(&path) {
            Ok(t) => t,
            Err(_) => default_rates(),
        }
    }

    /// Convention map: provider id -> environment lane.
    /// New providers default to `Api` (metered) so cost is never silently
    /// underreported.
    pub fn env_for_provider(provider: &str) -> Env {
        match provider {
            "ollama" | "local" => Env::Local,
            "claude-code" | "claude_code" | "codex-cli" | "codex_cli" | "codex" | "gemini-cli"
            | "gemini_cli" | "gemini" | "antigravity" => Env::Oauth,
            _ => Env::Api,
        }
    }

    /// Rough token estimator for adapters that don't report counts
    /// (the Ollama CLI subprocess being the main case today). ~3.7 chars/token
    /// is a common English approximation. **Best-effort.** Real implementation
    /// should call Ollama's HTTP API which reports `prompt_eval_count` and
    /// `eval_count` exactly.
    pub fn estimate_tokens(text: &str) -> i64 {
        let chars = text.chars().count() as f64;
        if chars == 0.0 {
            0
        } else {
            (chars / 3.7).ceil() as i64
        }
    }

    /// Compute costs with graceful zero-fallback when the rate entry is missing.
    /// Returns `(costs, found)` so callers can log unknown-rate cases without
    /// dropping the receipt.
    pub fn compute_or_zero(
        rates: &RateTable,
        env: Env,
        provider: &str,
        model: &str,
        tokens_in: i64,
        tokens_out: i64,
    ) -> (Costs, bool) {
        match rates.compute(env, provider, model, tokens_in, tokens_out) {
            Ok(c) => (c, true),
            Err(_) => (
                Costs {
                    actual_cad: 0.0,
                    counterfactual_cad: 0.0,
                },
                false,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_roundtrip() {
        for e in [Env::Local, Env::Oauth, Env::Api] {
            assert_eq!(Env::parse(e.as_str()).unwrap(), e);
        }
        assert!(matches!(
            Env::parse("invalid"),
            Err(ReceiptError::InvalidEnv(_))
        ));
    }

    #[test]
    fn cost_compute_oauth_zero_with_counterfactual() {
        let rates = r#"
            [rates.oauth.claude-code."claude-sonnet-4-6"]
            input_per_mtok_cad  = 0.0
            output_per_mtok_cad = 0.0

            [rates.oauth.claude-code."claude-sonnet-4-6".counterfactual]
            input_per_mtok_cad  = 4.05
            output_per_mtok_cad = 20.25
        "#;
        let table = RateTable::from_toml_str(rates).unwrap();
        let costs = table
            .compute(
                Env::Oauth,
                "claude-code",
                "claude-sonnet-4-6",
                1_000_000,
                200_000,
            )
            .unwrap();
        assert!((costs.actual_cad - 0.0).abs() < 1e-9);
        assert!((costs.counterfactual_cad - 8.10).abs() < 1e-9);
    }

    #[test]
    fn cost_compute_api_counterfactual_equals_actual_when_unset() {
        let rates = r#"
            [rates.api.openrouter."claude-3.7-sonnet"]
            input_per_mtok_cad  = 4.05
            output_per_mtok_cad = 20.25
        "#;
        let table = RateTable::from_toml_str(rates).unwrap();
        let costs = table
            .compute(Env::Api, "openrouter", "claude-3.7-sonnet", 1_000_000, 0)
            .unwrap();
        assert!((costs.actual_cad - 4.05).abs() < 1e-9);
        assert!((costs.counterfactual_cad - 4.05).abs() < 1e-9);
    }

    #[test]
    fn cost_missing_rate_surfaces_error() {
        let table = RateTable::default();
        let err = table
            .compute(Env::Api, "nowhere", "no-model", 100, 100)
            .unwrap_err();
        assert!(matches!(err, ReceiptError::RateNotFound { .. }));
    }
}
