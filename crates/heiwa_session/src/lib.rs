use anyhow::Result;
use heiwa_config::load as load_config;
use heiwa_embed::embed_and_store;
use heiwa_protocol::TranscriptBlock;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(unix)]
use tokio::net::UnixListener;
use uuid::Uuid;

pub mod migration;

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
    pub next_entry_id: u64,
    pub entries: Vec<TranscriptEntry>,
}

impl PersistedTranscript {
    pub fn empty(session_id: &str) -> Self {
        Self {
            version: PERSISTED_TRANSCRIPT_VERSION,
            session_id: session_id.to_string(),
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
    Ok(())
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
