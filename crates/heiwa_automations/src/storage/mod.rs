use crate::types::{
    Automation, AutomationId, AutomationStatus, Execution, ExecutionId, ExecutionStatus,
    TriggerConfig,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Durable local automation store backed by SQLite.
///
/// The schema stores canonical JSON records plus a few denormalized columns for
/// cheap scans. This keeps the schema stable while the domain model evolves.
#[derive(Clone)]
pub struct AutomationStore {
    conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
    state_dir: PathBuf,
}

impl AutomationStore {
    /// Open an automation DB at an explicit path (useful for tests).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db_path = path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create automation db parent {}", parent.display()))?;
        }
        let state_dir = db_path
            .parent()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let conn = Connection::open(&db_path)
            .with_context(|| format!("open automation db {}", db_path.display()))?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
            state_dir,
        };
        store.init()?;
        Ok(store)
    }

    /// Open the default DB under a Heiwa state directory.
    pub fn open_state_dir(state_dir: impl AsRef<Path>) -> Result<Self> {
        let state_dir = state_dir.as_ref().to_path_buf();
        Self::open(state_dir.join("automations/automations.sqlite3"))
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    fn init(&self) -> Result<()> {
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            PRAGMA foreign_keys=ON;

            CREATE TABLE IF NOT EXISTS automations (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                trigger_type TEXT,
                json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_executed_at TEXT,
                next_scheduled_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_automations_status_trigger
                ON automations(status, trigger_type);
            CREATE INDEX IF NOT EXISTS idx_automations_next_scheduled
                ON automations(next_scheduled_at);

            CREATE TABLE IF NOT EXISTS executions (
                id TEXT PRIMARY KEY,
                automation_id TEXT NOT NULL,
                status TEXT NOT NULL,
                json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT,
                FOREIGN KEY(automation_id) REFERENCES automations(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_executions_automation_created
                ON executions(automation_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_executions_status
                ON executions(status);
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_automation(&self, automation: &Automation) -> Result<()> {
        let trigger_type = trigger_type_text(automation.trigger_config.as_ref());
        let json = serde_json::to_string(automation)?;
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO automations (
                id, name, status, trigger_type, json, created_at, updated_at,
                last_executed_at, next_scheduled_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                name=excluded.name,
                status=excluded.status,
                trigger_type=excluded.trigger_type,
                json=excluded.json,
                updated_at=excluded.updated_at,
                last_executed_at=excluded.last_executed_at,
                next_scheduled_at=excluded.next_scheduled_at
            "#,
            params![
                automation.id.to_string(),
                automation.name,
                status_text(automation.status),
                trigger_type,
                json,
                automation.created_at.to_rfc3339(),
                automation.updated_at.to_rfc3339(),
                automation.last_executed_at.map(|dt| dt.to_rfc3339()),
                automation.next_scheduled_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn get_automation(&self, id: AutomationId) -> Result<Option<Automation>> {
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        let json: Option<String> = conn
            .query_row(
                "SELECT json FROM automations WHERE id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|raw| serde_json::from_str(&raw).context("decode automation json"))
            .transpose()
    }

    pub fn list_automations(&self) -> Result<Vec<Automation>> {
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        let mut stmt = conn.prepare("SELECT json FROM automations ORDER BY created_at ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        decode_rows(rows)
    }

    pub fn list_active_with_triggers(&self) -> Result<Vec<Automation>> {
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT json FROM automations WHERE status = 'active' AND trigger_type IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        decode_rows(rows)
    }

    pub fn delete_automation(&self, id: AutomationId) -> Result<bool> {
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        let changed = conn.execute(
            "DELETE FROM automations WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(changed > 0)
    }

    pub fn insert_execution(&self, execution: &Execution) -> Result<()> {
        let json = serde_json::to_string(execution)?;
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        conn.execute(
            r#"
            INSERT INTO executions (
                id, automation_id, status, json, created_at, started_at, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                execution.id.to_string(),
                execution.automation_id.to_string(),
                execution_status_text(execution.status),
                json,
                execution.created_at.to_rfc3339(),
                execution.started_at.map(|dt| dt.to_rfc3339()),
                execution.completed_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn update_execution(&self, execution: &Execution) -> Result<()> {
        let json = serde_json::to_string(execution)?;
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        conn.execute(
            r#"
            UPDATE executions
               SET status = ?2,
                   json = ?3,
                   started_at = ?4,
                   completed_at = ?5
             WHERE id = ?1
            "#,
            params![
                execution.id.to_string(),
                execution_status_text(execution.status),
                json,
                execution.started_at.map(|dt| dt.to_rfc3339()),
                execution.completed_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn get_execution(&self, id: ExecutionId) -> Result<Option<Execution>> {
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        let json: Option<String> = conn
            .query_row(
                "SELECT json FROM executions WHERE id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|raw| serde_json::from_str(&raw).context("decode execution json"))
            .transpose()
    }

    pub fn list_executions(&self, automation_id: AutomationId) -> Result<Vec<Execution>> {
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT json FROM executions WHERE automation_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![automation_id.to_string()], |row| {
            row.get::<_, String>(0)
        })?;
        decode_execution_rows(rows)
    }

    pub fn list_pending_executions(&self, limit: usize) -> Result<Vec<Execution>> {
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT json FROM executions WHERE status = 'pending' ORDER BY created_at ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit.max(1) as i64], |row| row.get::<_, String>(0))?;
        decode_execution_rows(rows)
    }

    /// Atomically recover expired running rows or fail exhausted rows.
    pub fn recover_stale_running_executions(
        &self,
        stale_before: DateTime<Utc>,
        max_recoveries: u32,
    ) -> Result<Vec<Execution>> {
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        let candidates = {
            let mut stmt = conn.prepare(
                "SELECT json FROM executions WHERE status = 'running' ORDER BY started_at ASC",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            decode_execution_rows(rows)?
        };
        let mut recovered = Vec::new();
        for mut execution in candidates {
            if execution
                .started_at
                .is_some_and(|started_at| started_at > stale_before)
            {
                continue;
            }
            let original_started_at = execution.started_at.map(|value| value.to_rfc3339());
            if execution.retry_count >= max_recoveries {
                execution.status = ExecutionStatus::Failed;
                execution.completed_at = Some(Utc::now());
                execution.error_message = Some("execution_lease_retry_limit_exceeded".to_string());
            } else {
                execution.status = ExecutionStatus::Pending;
                execution.started_at = None;
                execution.retry_count = execution.retry_count.saturating_add(1);
                execution.error_message = Some("requeued_after_expired_lease".to_string());
            }
            let updated_json = serde_json::to_string(&execution)?;
            let changed = conn.execute(
                r#"
                UPDATE executions
                   SET status = ?2,
                       json = ?3,
                       started_at = ?4,
                       completed_at = ?5
                 WHERE id = ?1
                   AND status = 'running'
                   AND (started_at = ?6 OR (started_at IS NULL AND ?6 IS NULL))
                "#,
                params![
                    execution.id.to_string(),
                    execution_status_text(execution.status),
                    updated_json,
                    execution.started_at.map(|value| value.to_rfc3339()),
                    execution.completed_at.map(|value| value.to_rfc3339()),
                    original_started_at,
                ],
            )?;
            if changed == 1 {
                recovered.push(execution);
            }
        }
        Ok(recovered)
    }

    /// Atomically move one pending execution to running.
    ///
    /// The `status = 'pending'` guard is the cross-process claim. Two runtime
    /// pumps may observe the same row, but only one SQLite update can win.
    pub fn claim_pending_execution(&self, id: ExecutionId) -> Result<Option<Execution>> {
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        let json: Option<String> = conn
            .query_row(
                "SELECT json FROM executions WHERE id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(raw) = json else {
            return Ok(None);
        };
        let mut execution: Execution =
            serde_json::from_str(&raw).context("decode automation execution json")?;
        if execution.status != ExecutionStatus::Pending {
            return Ok(None);
        }

        execution.status = ExecutionStatus::Running;
        execution.started_at = Some(Utc::now());
        let updated_json = serde_json::to_string(&execution)?;
        let changed = conn.execute(
            r#"
            UPDATE executions
               SET status = 'running',
                   json = ?2,
                   started_at = ?3
             WHERE id = ?1 AND status = 'pending'
            "#,
            params![
                id.to_string(),
                updated_json,
                execution.started_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        Ok((changed == 1).then_some(execution))
    }

    pub fn count_executions_since(
        &self,
        automation_id: AutomationId,
        since: DateTime<Utc>,
    ) -> Result<u32> {
        let conn = self.conn.lock().expect("automation store mutex poisoned");
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM executions WHERE automation_id = ?1 AND created_at >= ?2",
            params![automation_id.to_string(), since.to_rfc3339()],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as u32)
    }

    pub fn mark_next_scheduled(
        &self,
        id: AutomationId,
        next_scheduled_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        if let Some(mut automation) = self.get_automation(id)? {
            automation.next_scheduled_at = next_scheduled_at;
            automation.updated_at = Utc::now();
            self.upsert_automation(&automation)?;
        }
        Ok(())
    }

    pub fn mark_last_executed(&self, id: AutomationId, at: DateTime<Utc>) -> Result<()> {
        if let Some(mut automation) = self.get_automation(id)? {
            automation.last_executed_at = Some(at);
            automation.updated_at = Utc::now();
            self.upsert_automation(&automation)?;
        }
        Ok(())
    }
}

fn decode_rows(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<String>>,
) -> Result<Vec<Automation>> {
    let mut out = Vec::new();
    for row in rows {
        let raw = row?;
        out.push(serde_json::from_str(&raw).context("decode automation json")?);
    }
    Ok(out)
}

fn decode_execution_rows(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<String>>,
) -> Result<Vec<Execution>> {
    let mut out = Vec::new();
    for row in rows {
        let raw = row?;
        out.push(serde_json::from_str(&raw).context("decode execution json")?);
    }
    Ok(out)
}

fn trigger_type_text(config: Option<&TriggerConfig>) -> Option<&'static str> {
    match config {
        Some(TriggerConfig::Cron(_)) => Some("cron"),
        Some(TriggerConfig::FileWatch(_)) => Some("file_watch"),
        None => None,
    }
}

fn status_text(status: AutomationStatus) -> &'static str {
    match status {
        AutomationStatus::Active => "active",
        AutomationStatus::Paused => "paused",
        AutomationStatus::Disabled => "disabled",
    }
}

fn execution_status_text(status: crate::types::ExecutionStatus) -> &'static str {
    match status {
        crate::types::ExecutionStatus::Pending => "pending",
        crate::types::ExecutionStatus::Running => "running",
        crate::types::ExecutionStatus::AwaitingConfirmation => "awaiting_confirmation",
        crate::types::ExecutionStatus::Completed => "completed",
        crate::types::ExecutionStatus::Failed => "failed",
        crate::types::ExecutionStatus::Cancelled => "cancelled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ExecutionStatus, TriggerEventData};

    #[test]
    fn store_round_trips_automation_and_execution() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open_state_dir(tmp.path()).unwrap();
        let automation = Automation::new("daily brief".into(), "summarize today".into())
            .with_cron_trigger("0 9 * * *".into(), None)
            .activate();
        store.upsert_automation(&automation).unwrap();

        let loaded = store.get_automation(automation.id).unwrap().unwrap();
        assert_eq!(loaded.name, "daily brief");
        assert_eq!(loaded.status, AutomationStatus::Active);
        assert_eq!(store.list_active_with_triggers().unwrap().len(), 1);

        let execution = Execution {
            id: ExecutionId::new(),
            automation_id: automation.id,
            status: ExecutionStatus::Pending,
            trigger_data: Some(TriggerEventData::Cron {
                timestamp: Utc::now(),
                scheduled_time: Utc::now(),
            }),
            started_at: None,
            completed_at: None,
            error_message: None,
            retry_count: 0,
            created_at: Utc::now(),
        };
        store.insert_execution(&execution).unwrap();
        assert_eq!(store.list_executions(automation.id).unwrap().len(), 1);
    }
}
