use anyhow::Result;
use heiwa_protocol::TranscriptBlock;
use serde_json::Value;

use crate::{
    block_raw_char_len, PersistedTranscript, TranscriptEntry, PERSISTED_TRANSCRIPT_VERSION,
};

/// Parse a persisted transcript file, migrating from v0 if needed.
///
/// v0 shape: `{ "session_id": ..., "transcript": [TranscriptBlock, ...] }`
/// v1 shape: `{ "version": 1, "session_id": ..., "parent_session_id": optional, "next_entry_id": N, "entries": [...] }`
pub fn parse_persisted(session_id: &str, value: Value) -> Result<PersistedTranscript> {
    if value.get("version").is_some() && value.get("entries").is_some() {
        let persisted: PersistedTranscript = serde_json::from_value(value)?;
        return Ok(persisted);
    }
    migrate_v0(session_id, value)
}

fn migrate_v0(session_id: &str, value: Value) -> Result<PersistedTranscript> {
    let sid = value
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or(session_id)
        .to_string();

    let blocks: Vec<TranscriptBlock> = match value.get("transcript").cloned() {
        Some(v) => serde_json::from_value(v)?,
        None => Vec::new(),
    };

    let entries: Vec<TranscriptEntry> = blocks
        .into_iter()
        .enumerate()
        .map(|(idx, block)| TranscriptEntry {
            id: idx as u64,
            ts_unix_ms: 0,
            char_len: block_raw_char_len(&block),
            block,
            embedding_ref: None,
        })
        .collect();

    Ok(PersistedTranscript {
        version: PERSISTED_TRANSCRIPT_VERSION,
        session_id: sid,
        parent_session_id: None,
        next_entry_id: entries.len() as u64,
        entries,
    })
}
