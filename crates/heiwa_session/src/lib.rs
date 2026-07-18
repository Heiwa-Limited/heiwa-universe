use anyhow::Result;
use heiwa_config::load as load_config;
use heiwa_embed::embed_and_store;
use heiwa_protocol::TranscriptBlock;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(unix)]
use tokio::net::UnixListener;
#[cfg(unix)]
use uuid::Uuid;

pub mod migration;
pub mod operator;

pub const PERSISTED_TRANSCRIPT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub socket_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingRef {
    pub model: String,
    pub dim: u16,
    pub row_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub id: u64,
    pub ts_unix_ms: i64,
    pub char_len: usize,
    pub block: TranscriptBlock,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_ref: Option<EmbeddingRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTranscript {
    pub version: u32,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    pub next_entry_id: u64,
    pub entries: Vec<TranscriptEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSearchHit {
    pub session_id: String,
    pub entry_id: u64,
    pub role: String,
    pub content: String,
}

impl PersistedTranscript {
    pub fn empty(session_id: &str) -> Self {
        Self {
            version: PERSISTED_TRANSCRIPT_VERSION,
            session_id: session_id.to_string(),
            parent_session_id: None,
            next_entry_id: 0,
            entries: Vec::new(),
        }
    }

    pub fn blocks(&self) -> Vec<TranscriptBlock> {
        self.entries.iter().map(|e| e.block.clone()).collect()
    }
}

pub fn get_session_dir() -> PathBuf {
    load_config().paths.sessions_dir
}

fn get_transcript_path(session_id: &str) -> PathBuf {
    get_session_dir().join(format!("{}.json", session_id))
}

pub fn get_session_index_path() -> PathBuf {
    load_config()
        .paths
        .state_dir
        .join("state")
        .join("sessions.sqlite3")
}

pub fn block_raw_char_len(block: &TranscriptBlock) -> usize {
    match block {
        TranscriptBlock::User(text)
        | TranscriptBlock::Assistant(text)
        | TranscriptBlock::Evidence(text) => text.len(),
        TranscriptBlock::Tool(name, output) => name.len() + output.len(),
    }
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn load_transcript(session_id: &str) -> Result<PersistedTranscript> {
    let path = get_transcript_path(session_id);
    if !path.exists() {
        return Ok(PersistedTranscript::empty(session_id));
    }
    let content = fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;
    migration::parse_persisted(session_id, value)
}

pub fn save_entries(persisted: &PersistedTranscript) -> Result<()> {
    fs::create_dir_all(get_session_dir())?;
    let content = serde_json::to_string_pretty(persisted)?;
    fs::write(get_transcript_path(&persisted.session_id), content)?;
    if let Err(error) = sync_session_index(persisted) {
        eprintln!("session index sync skipped: {}", error);
    }
    Ok(())
}

pub fn set_parent_session_id(session_id: &str, parent_session_id: Option<String>) -> Result<()> {
    let mut persisted = load_transcript(session_id)?;
    persisted.parent_session_id = parent_session_id;
    save_entries(&persisted)
}

pub fn rebuild_session_index(session_id: &str) -> Result<()> {
    let persisted = load_transcript(session_id)?;
    sync_session_index(&persisted)
}

pub fn search_session_messages(
    session_id: Option<&str>,
    query: &str,
    limit: usize,
) -> Result<Vec<SessionSearchHit>> {
    let conn = open_session_index()?;
    ensure_session_index_schema(&conn)?;

    let limit = limit.clamp(1, 100) as i64;
    let mut hits = Vec::new();
    if let Some(session_id) = session_id {
        let mut stmt = conn.prepare(
            "SELECT session_id, entry_id, role, content
             FROM messages_fts
             WHERE messages_fts MATCH ?1 AND session_id = ?2
             ORDER BY rank
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![query, session_id, limit], row_to_search_hit)?;
        for row in rows {
            hits.push(row?);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT session_id, entry_id, role, content
             FROM messages_fts
             WHERE messages_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![query, limit], row_to_search_hit)?;
        for row in rows {
            hits.push(row?);
        }
    }
    Ok(hits)
}

fn row_to_search_hit(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionSearchHit> {
    Ok(SessionSearchHit {
        session_id: row.get(0)?,
        entry_id: row.get::<_, i64>(1)? as u64,
        role: row.get(2)?,
        content: row.get(3)?,
    })
}

fn open_session_index() -> Result<Connection> {
    let path = get_session_index_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(Connection::open(path)?)
}

fn ensure_session_index_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS sessions (
             id TEXT PRIMARY KEY,
             parent_session_id TEXT,
             updated_at_ms INTEGER NOT NULL,
             entry_count INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS messages (
             session_id TEXT NOT NULL,
             entry_id INTEGER NOT NULL,
             role TEXT NOT NULL,
             content TEXT NOT NULL,
             ts_unix_ms INTEGER NOT NULL,
             char_len INTEGER NOT NULL,
             PRIMARY KEY(session_id, entry_id)
         );
         CREATE INDEX IF NOT EXISTS idx_messages_session
             ON messages(session_id);
         CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts
             USING fts5(session_id UNINDEXED, entry_id UNINDEXED, role, content);",
    )?;
    Ok(())
}

fn sync_session_index(persisted: &PersistedTranscript) -> Result<()> {
    let mut conn = open_session_index()?;
    ensure_session_index_schema(&conn)?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO sessions (id, parent_session_id, updated_at_ms, entry_count)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
             parent_session_id = excluded.parent_session_id,
             updated_at_ms = excluded.updated_at_ms,
             entry_count = excluded.entry_count",
        params![
            persisted.session_id,
            persisted.parent_session_id,
            now_unix_ms(),
            persisted.entries.len() as i64,
        ],
    )?;
    tx.execute(
        "DELETE FROM messages WHERE session_id = ?1",
        params![persisted.session_id],
    )?;
    tx.execute(
        "DELETE FROM messages_fts WHERE session_id = ?1",
        params![persisted.session_id],
    )?;
    for entry in &persisted.entries {
        let (role, content) = block_index_text(&entry.block);
        tx.execute(
            "INSERT INTO messages
             (session_id, entry_id, role, content, ts_unix_ms, char_len)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                persisted.session_id,
                entry.id as i64,
                role,
                content,
                entry.ts_unix_ms,
                entry.char_len as i64,
            ],
        )?;
        tx.execute(
            "INSERT INTO messages_fts (session_id, entry_id, role, content)
             VALUES (?1, ?2, ?3, ?4)",
            params![persisted.session_id, entry.id as i64, role, content],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn block_index_text(block: &TranscriptBlock) -> (&'static str, String) {
    match block {
        TranscriptBlock::User(text) => ("user", text.clone()),
        TranscriptBlock::Assistant(text) => ("assistant", text.clone()),
        TranscriptBlock::Evidence(text) => ("evidence", text.clone()),
        TranscriptBlock::Tool(name, output) => ("tool", format!("{name}\n{output}")),
    }
}

/// Compat shim for callers that still pass `&[TranscriptBlock]`.
///
/// Reads the previously persisted entries to preserve IDs, timestamps, and
/// embedding refs for blocks already on disk. New blocks beyond the prior
/// length get fresh IDs from `next_entry_id`. Callers today are append-only,
/// so overlapping positions are assumed unchanged.
pub fn save_transcript(session_id: &str, blocks: &[TranscriptBlock]) -> Result<()> {
    let mut persisted = load_transcript(session_id)?;
    let prior_len = persisted.entries.len();

    if blocks.len() < prior_len {
        persisted.entries.truncate(blocks.len());
    }

    for block in blocks.iter().skip(prior_len) {
        let id = persisted.next_entry_id;
        persisted.next_entry_id += 1;
        persisted.entries.push(TranscriptEntry {
            id,
            ts_unix_ms: now_unix_ms(),
            char_len: block_raw_char_len(block),
            block: block.clone(),
            embedding_ref: None,
        });
    }
    persisted.version = PERSISTED_TRANSCRIPT_VERSION;
    persisted.session_id = session_id.to_string();
    save_entries(&persisted)
}

/// Append a single block, returning the populated entry.
pub fn append_entry(session_id: &str, block: TranscriptBlock) -> Result<TranscriptEntry> {
    let mut persisted = load_transcript(session_id)?;
    let mut entry = TranscriptEntry {
        id: persisted.next_entry_id,
        ts_unix_ms: now_unix_ms(),
        char_len: block_raw_char_len(&block),
        block,
        embedding_ref: None,
    };
    persisted.next_entry_id += 1;
    persisted.entries.push(entry.clone());
    persisted.version = PERSISTED_TRANSCRIPT_VERSION;
    persisted.session_id = session_id.to_string();
    save_entries(&persisted)?;

    match embed_and_store(session_id, entry.id, block_text(&entry.block)) {
        Ok(Some(reference)) => {
            entry.embedding_ref = Some(EmbeddingRef {
                model: reference.model,
                dim: reference.dim,
                row_id: reference.row_id,
            });
            if let Some(last) = persisted.entries.last_mut() {
                last.embedding_ref = entry.embedding_ref.clone();
            }
            save_entries(&persisted)?;
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("embedding append skipped: {}", error);
        }
    }

    Ok(entry)
}

fn block_text(block: &TranscriptBlock) -> &str {
    match block {
        TranscriptBlock::User(text)
        | TranscriptBlock::Assistant(text)
        | TranscriptBlock::Evidence(text) => text.as_str(),
        TranscriptBlock::Tool(_, output) => output.as_str(),
    }
}

#[cfg(unix)]
pub fn start_daemon() -> Result<SessionInfo> {
    let session_id = Uuid::new_v4().to_string();
    let session_dir = get_session_dir();
    fs::create_dir_all(&session_dir)?;

    let socket_path = session_dir.join(format!("{}.sock", session_id));
    let socket_path_clone = socket_path.clone();

    tokio::spawn(async move {
        let listener = UnixListener::bind(&socket_path_clone).expect("failed to bind socket");
        loop {
            match listener.accept().await {
                Ok((_stream, _addr)) => {
                    // Handle control connection
                }
                Err(e) => eprintln!("Accept error: {}", e),
            }
        }
    });

    Ok(SessionInfo {
        session_id,
        socket_path,
    })
}

#[cfg(not(unix))]
pub fn start_daemon() -> Result<SessionInfo> {
    Err(anyhow::anyhow!(
        "session daemon sockets are not supported on this platform yet"
    ))
}

pub fn attach_session(_session_id: &str) -> Result<()> {
    Ok(())
}

pub struct PtySession {
    pub master: Box<dyn portable_pty::MasterPty + Send>,
}

impl PtySession {
    pub fn new() -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = if cfg!(target_os = "windows") {
            "cmd.exe"
        } else {
            "bash"
        };

        let cmd = CommandBuilder::new(shell);
        let _child = pair.slave.spawn_command(cmd)?;

        Ok(Self {
            master: pair.master,
        })
    }
}
