//! Rebuildable SQLite FTS and embedding projections over operator events.

use std::fs;
use std::path::Path;

use anyhow::Result;
use heiwa_embed::{clear_embeddings, embed_and_store};
use heiwa_evidence::OPERATOR_EVENT_SCHEMA_VERSION;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use crate::operator::OperatorSessionService;
use crate::{get_session_index_path, SessionSearchHit};

/// Optional derived-index writer. Failures are reported, never journal failures.
pub trait EmbeddingSink: Send + Sync {
    /// Begin replacing the complete derived semantic projection.
    fn begin_replace(&self) -> Result<()> {
        Ok(())
    }
    /// Returns whether a vector was stored. Disabled embedding backends return
    /// `false` without counting as a failure.
    fn upsert_text(&self, thread_id: &str, event_id: &str, text: &str) -> Result<bool>;
    /// Finalize replacement only after every eligible event was offered.
    fn finalize_replace(&self) -> Result<()> {
        Ok(())
    }
}

/// Configured embedding projection used by the installed service.
pub struct ProductionEmbeddingSink;

impl EmbeddingSink for ProductionEmbeddingSink {
    fn begin_replace(&self) -> Result<()> {
        clear_embeddings()
    }

    fn upsert_text(&self, thread_id: &str, event_id: &str, text: &str) -> Result<bool> {
        embed_and_store(thread_id, operator_event_key(event_id), text)
            .map(|stored| stored.is_some())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexReport {
    pub fts_rows: usize,
    pub embedded_rows: usize,
    pub embedding_failures: usize,
}

pub fn rebuild_operator_indexes(
    service: &OperatorSessionService,
    sink: &dyn EmbeddingSink,
) -> Result<IndexReport> {
    rebuild_operator_indexes_at(service, sink, &get_session_index_path())
}

/// Injected index-root seam for tests and sandboxed callers.
pub fn rebuild_operator_indexes_at(
    service: &OperatorSessionService,
    sink: &dyn EmbeddingSink,
    index_path: &Path,
) -> Result<IndexReport> {
    if let Some(parent) = index_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(index_path)?;
    let tx = conn.transaction()?;
    // These are derived tables. Recreate them so pre-cutover schemas cannot
    // leak legacy numeric keys into the event-keyed projection.
    tx.execute_batch(&format!(
        "DROP TABLE IF EXISTS messages_fts;
         DROP TABLE IF EXISTS messages;
         DROP TABLE IF EXISTS indexed_operator_events;
         {SCHEMA_SQL}"
    ))?;

    let mut fts_rows = 0;
    let mut embedded_rows = 0;
    let mut embedding_failures = 0;
    let embedding_ready = sink.begin_replace().is_ok();
    let mut cursor = None;
    loop {
        let page = service.journal_page_after(cursor.as_deref(), 256)?;
        let event_count = page.events.len();
        let next_cursor = page.next_cursor.clone();
        for row in page.events {
            let inserted_event = tx.execute(
                "INSERT OR IGNORE INTO indexed_operator_events (event_id) VALUES (?1)",
                params![&row.event.event_id],
            )?;
            if inserted_event == 0 || row.event.schema_version != OPERATOR_EVENT_SCHEMA_VERSION {
                continue;
            }
            let Some((role, text, embed)) = event_text(&row.event) else {
                continue;
            };
            let key = operator_event_key(&row.event.event_id);
            tx.execute(
                "INSERT INTO messages (thread_id, event_id, entry_id, role, content)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    &row.event.thread_id,
                    &row.event.event_id,
                    key as i64,
                    role,
                    &text
                ],
            )?;
            tx.execute(
                "INSERT INTO messages_fts (thread_id, event_id, entry_id, role, content)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    &row.event.thread_id,
                    &row.event.event_id,
                    key as i64,
                    role,
                    &text
                ],
            )?;
            fts_rows += 1;
            if embed {
                if !embedding_ready {
                    embedding_failures += 1;
                } else {
                    match sink.upsert_text(&row.event.thread_id, &row.event.event_id, &text) {
                        Ok(true) => embedded_rows += 1,
                        Ok(false) => {}
                        Err(_) => embedding_failures += 1,
                    }
                }
            }
        }
        if event_count == 0 || next_cursor == cursor {
            break;
        }
        cursor = next_cursor;
    }
    tx.commit()?;

    if embedding_ready && sink.finalize_replace().is_err() {
        embedding_failures += 1;
    }
    Ok(IndexReport {
        fts_rows,
        embedded_rows,
        embedding_failures,
    })
}

pub fn search_session_messages_at(
    index_path: &Path,
    session_id: Option<&str>,
    query: &str,
    limit: usize,
) -> Result<Vec<SessionSearchHit>> {
    let conn = Connection::open(index_path)?;
    ensure_schema(&conn)?;
    let limit = limit.clamp(1, 100) as i64;
    let mut hits = Vec::new();
    let sql = if session_id.is_some() {
        "SELECT thread_id, event_id, entry_id, role, content FROM messages_fts WHERE messages_fts MATCH ?1 AND thread_id = ?2 ORDER BY rank LIMIT ?3"
    } else {
        "SELECT thread_id, event_id, entry_id, role, content FROM messages_fts WHERE messages_fts MATCH ?1 ORDER BY rank LIMIT ?2"
    };
    let mut statement = conn.prepare(sql)?;
    if let Some(session_id) = session_id {
        for row in statement.query_map(params![query, session_id, limit], row_to_hit)? {
            hits.push(row?);
        }
    } else {
        for row in statement.query_map(params![query, limit], row_to_hit)? {
            hits.push(row?);
        }
    }
    Ok(hits)
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(())
}

const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS messages (
             thread_id TEXT NOT NULL,
             event_id TEXT NOT NULL UNIQUE,
             entry_id INTEGER NOT NULL,
             role TEXT NOT NULL,
             content TEXT NOT NULL,
             PRIMARY KEY(thread_id, event_id)
         );
         CREATE TABLE IF NOT EXISTS indexed_operator_events (
             event_id TEXT PRIMARY KEY
         );
         CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts
             USING fts5(thread_id UNINDEXED, event_id UNINDEXED, entry_id UNINDEXED, role, content);";

fn row_to_hit(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionSearchHit> {
    Ok(SessionSearchHit {
        session_id: row.get(0)?,
        event_id: row.get(1)?,
        entry_id: row.get::<_, i64>(2)? as u64,
        role: row.get(3)?,
        content: row.get(4)?,
    })
}

/// Stable numeric join key shared by FTS and embedding projections for one
/// durable operator event.
pub fn operator_event_key(event_id: &str) -> u64 {
    let digest = Sha256::digest(event_id.as_bytes());
    u64::from_be_bytes(digest[..8].try_into().expect("sha256 prefix"))
}

fn event_text(event: &heiwa_evidence::OperatorEvent) -> Option<(&'static str, String, bool)> {
    use heiwa_evidence::OperatorEventType;
    match event.event_type {
        OperatorEventType::UserMessage => event
            .payload
            .get("text")?
            .as_str()
            .map(|text| ("user", text.to_string(), true)),
        OperatorEventType::AssistantCompleted => event
            .payload
            .get("text")?
            .as_str()
            .map(|text| ("assistant", text.to_string(), true)),
        OperatorEventType::ToolCallCompleted => {
            let output = event.payload.get("output")?.as_str()?;
            let name = event
                .payload
                .get("name")
                .and_then(|name| name.as_str())
                .unwrap_or("tool");
            Some(("tool", format!("{name}\n{output}"), false))
        }
        OperatorEventType::ReceiptLinked => event
            .payload
            .get("text")?
            .as_str()
            .map(|text| ("evidence", text.to_string(), false)),
        _ => None,
    }
}
