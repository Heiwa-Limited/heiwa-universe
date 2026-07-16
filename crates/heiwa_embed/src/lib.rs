#[cfg(feature = "lance")]
pub mod lance_store;
#[cfg(feature = "lance")]
pub use lance_store::LanceVectorStore;

use anyhow::{Context, Result};
use heiwa_config::load as load_config;
use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub struct StoredEmbeddingRef {
    pub row_id: u64,
    pub model: String,
    pub dim: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimilarEntry {
    pub row_id: u64,
    pub entry_id: u64,
    pub score: f32,
    pub model: String,
}

#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    #[serde(default)]
    embedding: Vec<f32>,
}

pub fn embed_and_store(
    session_id: &str,
    entry_id: u64,
    text: &str,
) -> Result<Option<StoredEmbeddingRef>> {
    let config = load_config();
    if !config.embedding.enabled {
        return Ok(None);
    }

    let Some(ollama_url) = config.embedding.ollama_url.clone() else {
        return Ok(None);
    };

    let client = OllamaEmbeddingClient::new(
        ollama_url,
        config.embedding.model.clone(),
        config.embedding.request_timeout_ms,
    )?;
    let vector = client.embed(text)?;
    if vector.is_empty() {
        return Ok(None);
    }

    let store = SqliteVectorStore::open(&config.embedding.sqlite_path)?;
    let row_id = store.upsert(session_id, entry_id, &config.embedding.model, &vector)?;
    Ok(Some(StoredEmbeddingRef {
        row_id,
        model: config.embedding.model,
        dim: vector.len().min(u16::MAX as usize) as u16,
    }))
}

pub struct OllamaEmbeddingClient {
    client: Client,
    endpoint: String,
    model: String,
}

impl OllamaEmbeddingClient {
    pub fn new(endpoint: String, model: String, timeout_ms: u64) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()?;
        Ok(Self {
            client,
            endpoint,
            model,
        })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/api/embeddings", self.endpoint.trim_end_matches('/'));
        let response = self
            .client
            .post(url)
            .json(&serde_json::json!({
                "model": self.model,
                "prompt": text,
            }))
            .send()
            .context("embedding request failed")?;

        if !response.status().is_success() {
            anyhow::bail!("embedding request returned HTTP {}", response.status());
        }

        let payload: OllamaEmbeddingResponse = response.json()?;
        Ok(payload.embedding)
    }
}

pub struct SqliteVectorStore {
    conn: Connection,
}

impl SqliteVectorStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS transcript_embeddings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                entry_id INTEGER NOT NULL,
                model TEXT NOT NULL,
                dim INTEGER NOT NULL,
                vector_json TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                UNIQUE(session_id, entry_id)
            );
            CREATE INDEX IF NOT EXISTS idx_transcript_embeddings_session
            ON transcript_embeddings(session_id);",
        )?;
        Ok(())
    }

    pub fn upsert(
        &self,
        session_id: &str,
        entry_id: u64,
        model: &str,
        vector: &[f32],
    ) -> Result<u64> {
        let now = now_unix_ms();
        let vector_json = serde_json::to_string(vector)?;
        self.conn.execute(
            "INSERT INTO transcript_embeddings
                (session_id, entry_id, model, dim, vector_json, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(session_id, entry_id)
             DO UPDATE SET
                model = excluded.model,
                dim = excluded.dim,
                vector_json = excluded.vector_json,
                updated_at_ms = excluded.updated_at_ms",
            params![
                session_id,
                entry_id as i64,
                model,
                vector.len() as i64,
                vector_json,
                now
            ],
        )?;

        let row_id = self
            .conn
            .query_row(
                "SELECT id FROM transcript_embeddings WHERE session_id = ?1 AND entry_id = ?2",
                params![session_id, entry_id as i64],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or_default();
        Ok(row_id as u64)
    }

    pub fn top_k_similar(
        &self,
        session_id: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<SimilarEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, entry_id, model, vector_json
             FROM transcript_embeddings
             WHERE session_id = ?1",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (row_id, entry_id, model, vector_json) = row?;
            let vector: Vec<f32> = serde_json::from_str(&vector_json)?;
            let score = cosine_similarity(query, &vector);
            results.push(SimilarEntry {
                row_id: row_id as u64,
                entry_id: entry_id as u64,
                score,
                model,
            });
        }

        results.sort_by(|a, b| b.score.total_cmp(&a.score));
        results.truncate(limit);
        Ok(results)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (left, right) in a.iter().zip(b.iter()) {
        dot += left * right;
        norm_a += left * left;
        norm_b += right * right;
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a.sqrt() * norm_b.sqrt())
    }
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn temp_store() -> (TempDir, SqliteVectorStore, PathBuf) {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("memory.sqlite3");
        let store = SqliteVectorStore::open(&path).expect("open store");
        (dir, store, path)
    }

    #[test]
    fn sqlite_store_upserts_and_queries_similarity() {
        let (_dir, store, _path) = temp_store();
        let row_a = store
            .upsert("default", 0, "qwen3-embedding:0.6b", &[1.0, 0.0, 0.0])
            .expect("upsert a");
        let row_b = store
            .upsert("default", 1, "qwen3-embedding:0.6b", &[0.0, 1.0, 0.0])
            .expect("upsert b");

        assert!(row_a > 0);
        assert!(row_b > 0);

        let results = store
            .top_k_similar("default", &[0.9, 0.1, 0.0], 2)
            .expect("query");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].entry_id, 0);
        assert!(results[0].score >= results[1].score);
    }
}
