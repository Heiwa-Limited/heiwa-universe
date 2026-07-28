#[cfg(feature = "lance")]
pub mod lance_store;
#[cfg(feature = "lance")]
pub use lance_store::LanceVectorStore;

use anyhow::{Context, Result};
use heiwa_config::{load as load_config, EmbedBackend, EmbeddingConfig};
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

/// One stored embedding, the unit of rebuild and migration. The vector index
/// is derived state: any backend can be rebuilt from a stream of these.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingRow {
    pub session_id: String,
    pub entry_id: u64,
    pub model: String,
    pub vector: Vec<f32>,
}

/// Config-selected vector store. SQLite is the default hot-state store;
/// Lance (feature `"lance"`) is the derived recall index. If the config asks
/// for Lance but the binary was built without the feature, we fall back to
/// SQLite rather than dropping embeddings — the index is rebuildable, so
/// availability wins over backend fidelity.
pub enum VectorBackend {
    Sqlite(SqliteVectorStore),
    #[cfg(feature = "lance")]
    Lance(LanceVectorStore),
}

impl VectorBackend {
    pub fn open_from_config(config: &EmbeddingConfig) -> Result<Self> {
        match config.backend {
            EmbedBackend::Sqlite => Ok(Self::Sqlite(SqliteVectorStore::open(&config.sqlite_path)?)),
            #[cfg(feature = "lance")]
            EmbedBackend::Lance => Ok(Self::Lance(LanceVectorStore::open(
                &config.lance_path,
                config.dim,
            )?)),
            #[cfg(not(feature = "lance"))]
            EmbedBackend::Lance => {
                eprintln!(
                    "heiwa_embed: config requests lance backend but this build lacks the \
                     'lance' feature; falling back to sqlite at {}",
                    config.sqlite_path.display()
                );
                Ok(Self::Sqlite(SqliteVectorStore::open(&config.sqlite_path)?))
            }
        }
    }

    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Sqlite(_) => "sqlite",
            #[cfg(feature = "lance")]
            Self::Lance(_) => "lance",
        }
    }

    pub fn upsert(
        &self,
        session_id: &str,
        entry_id: u64,
        model: &str,
        vector: &[f32],
    ) -> Result<u64> {
        match self {
            Self::Sqlite(store) => store.upsert(session_id, entry_id, model, vector),
            #[cfg(feature = "lance")]
            Self::Lance(store) => {
                store.upsert(session_id, entry_id, model, vector)?;
                // Lance has no rowid; the (session, entry) key is the stable
                // reference, mirroring what `top_k_similar` reports back.
                Ok(entry_id)
            }
        }
    }

    pub fn top_k_similar(
        &self,
        session_id: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<SimilarEntry>> {
        match self {
            Self::Sqlite(store) => store.top_k_similar(session_id, query, limit),
            #[cfg(feature = "lance")]
            Self::Lance(store) => store.top_k_similar(session_id, query, limit),
        }
    }

    pub fn rebuild_from(&self, rows: Vec<EmbeddingRow>) -> Result<usize> {
        match self {
            Self::Sqlite(store) => store.rebuild_from(rows),
            #[cfg(feature = "lance")]
            Self::Lance(store) => store.rebuild_from(rows.into_iter()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingReplacementReport {
    pub stored_rows: usize,
    pub failures: usize,
}

/// Generate every available vector before replacing the derived backend.
/// Existing rows remain untouched if vector generation itself fails.
pub fn replace_embeddings_from_texts(
    texts: &[(String, u64, String)],
) -> Result<EmbeddingReplacementReport> {
    let config = load_config();
    let store = VectorBackend::open_from_config(&config.embedding)?;
    if !config.embedding.enabled || config.embedding.ollama_url.is_none() {
        return Ok(EmbeddingReplacementReport {
            stored_rows: store.rebuild_from(Vec::new())?,
            failures: 0,
        });
    }
    let client = OllamaEmbeddingClient::new(
        config.embedding.ollama_url.clone().unwrap(),
        config.embedding.model.clone(),
        config.embedding.request_timeout_ms,
    )?;
    let mut rows = Vec::new();
    let mut failures = 0;
    for (session_id, entry_id, text) in texts {
        match client.embed(text) {
            Ok(vector) if !vector.is_empty() => rows.push(EmbeddingRow {
                session_id: session_id.clone(),
                entry_id: *entry_id,
                model: config.embedding.model.clone(),
                vector,
            }),
            Ok(_) => failures += 1,
            Err(_) => failures += 1,
        }
    }
    let stored_rows = store.rebuild_from(rows)?;
    Ok(EmbeddingReplacementReport {
        stored_rows,
        failures,
    })
}

/// Copy every SQLite-stored embedding into a Lance store. One-way, additive:
/// rows already in Lance under the same (session, entry) key are replaced.
#[cfg(feature = "lance")]
pub fn migrate_sqlite_embeddings(
    sqlite: &SqliteVectorStore,
    lance: &LanceVectorStore,
) -> Result<usize> {
    let rows = sqlite.all_embeddings()?;
    let count = rows.len();
    for row in rows {
        lance.upsert(&row.session_id, row.entry_id, &row.model, &row.vector)?;
    }
    Ok(count)
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

    let store = VectorBackend::open_from_config(&config.embedding)?;
    let row_id = store.upsert(session_id, entry_id, &config.embedding.model, &vector)?;
    Ok(Some(StoredEmbeddingRef {
        row_id,
        model: config.embedding.model,
        dim: vector.len().min(u16::MAX as usize) as u16,
    }))
}

/// Clear the configured derived embedding backend before a streamed rebuild.
pub fn clear_embeddings() -> Result<()> {
    let config = load_config();
    let store = VectorBackend::open_from_config(&config.embedding)?;
    store.rebuild_from(Vec::new())?;
    Ok(())
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

    /// Dump every stored embedding, for backend rebuilds and migrations.
    pub fn all_embeddings(&self) -> Result<Vec<EmbeddingRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, entry_id, model, vector_json
             FROM transcript_embeddings
             ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (session_id, entry_id, model, vector_json) = row?;
            results.push(EmbeddingRow {
                session_id,
                entry_id: entry_id as u64,
                model,
                vector: serde_json::from_str(&vector_json)?,
            });
        }
        Ok(results)
    }

    pub fn rebuild_from(&self, rows: Vec<EmbeddingRow>) -> Result<usize> {
        self.conn.execute("DELETE FROM transcript_embeddings", [])?;
        let count = rows.len();
        for row in rows {
            self.upsert(&row.session_id, row.entry_id, &row.model, &row.vector)?;
        }
        Ok(count)
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

    #[test]
    fn sqlite_store_dumps_all_embeddings_for_rebuild_and_migration() {
        let (_dir, store, _path) = temp_store();
        store.upsert("a", 1, "m", &[1.0, 0.0]).expect("upsert 1");
        store.upsert("b", 2, "m", &[0.0, 1.0]).expect("upsert 2");

        let rows = store.all_embeddings().expect("dump");
        assert_eq!(rows.len(), 2);
        let entry = rows
            .iter()
            .find(|row| row.session_id == "a")
            .expect("row a");
        assert_eq!(entry.entry_id, 1);
        assert_eq!(entry.model, "m");
        assert_eq!(entry.vector, vec![1.0, 0.0]);
    }

    fn test_config(dir: &Path, backend: EmbedBackend) -> heiwa_config::EmbeddingConfig {
        heiwa_config::EmbeddingConfig {
            enabled: true,
            model: "test-model".to_string(),
            ollama_url: None,
            backend,
            sqlite_path: dir.join("memory.sqlite3"),
            lance_path: dir.join("lance"),
            dim: 3,
            request_timeout_ms: 100,
        }
    }

    #[test]
    fn vector_backend_dispatches_to_sqlite_from_config() {
        let dir = TempDir::new().expect("tempdir");
        let config = test_config(dir.path(), EmbedBackend::Sqlite);
        let store = VectorBackend::open_from_config(&config).expect("open");
        assert_eq!(store.backend_name(), "sqlite");
        store
            .upsert("s", 1, "test-model", &[1.0, 0.0, 0.0])
            .expect("upsert");
        let results = store
            .top_k_similar("s", &[1.0, 0.0, 0.0], 1)
            .expect("query");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry_id, 1);
    }

    #[cfg(not(feature = "lance"))]
    #[test]
    fn lance_backend_without_feature_falls_back_to_sqlite() {
        let dir = TempDir::new().expect("tempdir");
        let config = test_config(dir.path(), EmbedBackend::Lance);
        let store = VectorBackend::open_from_config(&config).expect("open");
        assert_eq!(store.backend_name(), "sqlite");
    }

    #[cfg(feature = "lance")]
    #[test]
    fn lance_backend_with_feature_opens_lance_store() {
        let dir = TempDir::new().expect("tempdir");
        let config = test_config(dir.path(), EmbedBackend::Lance);
        let store = VectorBackend::open_from_config(&config).expect("open");
        assert_eq!(store.backend_name(), "lance");
        store
            .upsert("s", 1, "test-model", &[1.0, 0.0, 0.0])
            .expect("upsert");
        let results = store
            .top_k_similar("s", &[1.0, 0.0, 0.0], 1)
            .expect("query");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry_id, 1);
    }

    #[cfg(feature = "lance")]
    #[test]
    fn migrate_sqlite_embeddings_moves_all_rows_into_lance() {
        let dir = TempDir::new().expect("tempdir");
        let sqlite = SqliteVectorStore::open(&dir.path().join("memory.sqlite3")).expect("sqlite");
        sqlite.upsert("s", 1, "m", &[1.0, 0.0, 0.0]).expect("row 1");
        sqlite.upsert("s", 2, "m", &[0.0, 1.0, 0.0]).expect("row 2");
        sqlite
            .upsert("other", 3, "m", &[0.0, 0.0, 1.0])
            .expect("row 3");

        let lance = LanceVectorStore::open(&dir.path().join("lance"), 3).expect("lance");
        let migrated = migrate_sqlite_embeddings(&sqlite, &lance).expect("migrate");
        assert_eq!(migrated, 3);

        let results = lance
            .top_k_similar("s", &[1.0, 0.0, 0.0], 10)
            .expect("query");
        assert_eq!(results.len(), 2, "session filter respected after migration");
        assert_eq!(results[0].entry_id, 1);
    }
}
