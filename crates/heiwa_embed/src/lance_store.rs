//! Lance-backed vector store (feature = "lance").
//!
//! Backend pivot 2026-07-15: Lance is the derived local recall index over
//! Heiwa's text evidence truth. This store mirrors `SqliteVectorStore`'s
//! sync interface (an internal tokio runtime drives lancedb's async API) so
//! callers can swap stores without changing shape. Data lives in a Lance
//! dataset directory; it is rebuildable from the JSONL/transcript truth and
//! must never be git-synced.

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use futures::TryStreamExt;
use lancedb::arrow::arrow_array::{
    types::Float32Type, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator,
    RecordBatchReader, StringArray, UInt64Array,
};
use lancedb::arrow::arrow_schema::{ArrowError, DataType, Field, Schema};
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, DistanceType, Table};

use crate::{EmbeddingRow, SimilarEntry};

const TABLE_NAME: &str = "transcript_embeddings";

/// Drive a lance future to completion from synchronous code without ever
/// panicking on nested-runtime entry. Callers may be plain threads, tokio
/// multi-thread workers, or a current-thread runtime:
///
/// - outside any runtime: block on our dedicated runtime directly;
/// - inside a multi-thread runtime: `block_in_place` lifts the "cannot block
///   a worker" restriction, then we block on our own runtime;
/// - inside a current-thread runtime (where `block_in_place` itself panics):
///   run the future on a scoped thread against our runtime and join.
fn run_blocking<F>(runtime: &tokio::runtime::Runtime, future: F) -> F::Output
where
    F: std::future::Future + Send,
    F::Output: Send,
{
    match tokio::runtime::Handle::try_current() {
        Err(_) => runtime.block_on(future),
        Ok(handle) => match handle.runtime_flavor() {
            tokio::runtime::RuntimeFlavor::CurrentThread => std::thread::scope(|scope| {
                scope
                    .spawn(|| runtime.block_on(future))
                    .join()
                    .expect("lance blocking thread panicked")
            }),
            _ => tokio::task::block_in_place(|| runtime.block_on(future)),
        },
    }
}

pub struct LanceVectorStore {
    conn: Connection,
    /// Dataset directory, retained so `table()` can distinguish a genuinely
    /// absent table from a corrupt one. lancedb's own classification is not
    /// portable - see the note there.
    dir: std::path::PathBuf,
    /// Dedicated runtime driving lancedb's async API. `Option` only so Drop
    /// can take it: dropping a `Runtime` inside an async context panics, so
    /// Drop hands it to `shutdown_background()` instead when a caller drops
    /// the store from within tokio.
    runtime: Option<tokio::runtime::Runtime>,
    dim: i32,
}

impl Drop for LanceVectorStore {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            if tokio::runtime::Handle::try_current().is_ok() {
                runtime.shutdown_background();
            }
            // Outside a runtime the normal (blocking) drop is fine.
        }
    }
}

impl LanceVectorStore {
    /// Open (or create) a Lance dataset directory. `dim` is the embedding
    /// dimensionality and is fixed per store (e.g. 1024 for
    /// qwen3-embedding:0.6b).
    pub fn open(dir: &Path, dim: usize) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        let uri = dir
            .to_str()
            .ok_or_else(|| anyhow!("non-utf8 lance dataset path"))?
            .to_string();
        let conn = run_blocking(&runtime, async { lancedb::connect(&uri).execute().await })?;
        Ok(Self {
            conn,
            dir: dir.to_path_buf(),
            runtime: Some(runtime),
            dim: dim as i32,
        })
    }

    fn runtime(&self) -> &tokio::runtime::Runtime {
        self.runtime.as_ref().expect("runtime present until drop")
    }

    fn schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("session_id", DataType::Utf8, false),
            Field::new("entry_id", DataType::UInt64, false),
            Field::new("model", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.dim,
                ),
                false,
            ),
        ]))
    }

    fn batch(
        &self,
        session_id: &str,
        entry_id: u64,
        model: &str,
        vector: &[f32],
    ) -> Result<RecordBatch> {
        if vector.len() as i32 != self.dim {
            return Err(anyhow!(
                "vector dim {} does not match store dim {}",
                vector.len(),
                self.dim
            ));
        }
        let vectors = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vec![Some(vector.iter().copied().map(Some).collect::<Vec<_>>())],
            self.dim,
        );
        RecordBatch::try_new(
            self.schema(),
            vec![
                Arc::new(StringArray::from(vec![session_id.to_string()])),
                Arc::new(UInt64Array::from(vec![entry_id])),
                Arc::new(StringArray::from(vec![model.to_string()])),
                Arc::new(vectors),
            ],
        )
        .context("building record batch")
    }

    async fn table(&self) -> Result<Table> {
        match self.conn.open_table(TABLE_NAME).execute().await {
            Ok(table) => Ok(table),
            // Only a missing table means "create it". Any other failure
            // (corruption, permissions, version skew) must surface as-is —
            // masking it behind a create attempt would hide real damage.
            Err(lancedb::Error::TableNotFound { .. }) => {
                // Do not trust TableNotFound to mean "absent". A stray file
                // where the table directory belongs is reported as an IO error
                // on Unix but as TableNotFound on Windows, so on Windows this
                // branch would try to create over the corruption and fail with
                // "Cannot create a file when that file already exists" - losing
                // the real diagnosis, which is exactly what the surrounding
                // comment says must not happen. Confirm absence ourselves.
                let table_path = self.dir.join(format!("{TABLE_NAME}.lance"));
                if table_path.exists() {
                    return Err(anyhow!(
                        "opening lance table: {} exists but is not a readable \
                         lance table",
                        table_path.display()
                    ));
                }
                let empty: Box<dyn RecordBatchReader + Send> = Box::new(RecordBatchIterator::new(
                    std::iter::empty::<Result<RecordBatch, ArrowError>>(),
                    self.schema(),
                ));
                self.conn
                    .create_table(TABLE_NAME, empty)
                    .execute()
                    .await
                    .map_err(|error| anyhow!("creating lance table: {error}"))
            }
            Err(error) => Err(anyhow!("opening lance table: {error}")),
        }
    }

    /// Drop everything and re-ingest from the source of truth. The Lance
    /// index is derived state; this is the recovery path when the index is
    /// lost, corrupt, or the embedding model changed.
    pub fn rebuild_from<I>(&self, rows: I) -> Result<usize>
    where
        I: Iterator<Item = EmbeddingRow>,
    {
        run_blocking(self.runtime(), async {
            match self.conn.drop_table(TABLE_NAME, &[]).await {
                Ok(()) | Err(lancedb::Error::TableNotFound { .. }) => Ok(()),
                Err(error) => Err(anyhow!("dropping lance table for rebuild: {error}")),
            }
        })?;
        let mut count = 0;
        for row in rows {
            self.upsert(&row.session_id, row.entry_id, &row.model, &row.vector)?;
            count += 1;
        }
        Ok(count)
    }

    /// Insert or replace the embedding for (session_id, entry_id).
    pub fn upsert(
        &self,
        session_id: &str,
        entry_id: u64,
        model: &str,
        vector: &[f32],
    ) -> Result<()> {
        let batch = self.batch(session_id, entry_id, model, vector)?;
        let schema = self.schema();
        run_blocking(self.runtime(), async {
            let table = self.table().await?;
            let mut merge = table.merge_insert(&["session_id", "entry_id"]);
            merge
                .when_matched_update_all(None)
                .when_not_matched_insert_all();
            let reader: Box<dyn RecordBatchReader + Send> = Box::new(RecordBatchIterator::new(
                vec![Ok::<RecordBatch, ArrowError>(batch)].into_iter(),
                schema,
            ));
            merge
                .execute(reader)
                .await
                .map_err(|error| anyhow!("lance merge insert: {error}"))?;
            Ok(())
        })
    }

    /// Cosine top-k over one session's embeddings. Scores are cosine
    /// similarity (1 - cosine distance) for parity with `SqliteVectorStore`.
    pub fn top_k_similar(
        &self,
        session_id: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<SimilarEntry>> {
        run_blocking(self.runtime(), async {
            let table = self.table().await?;
            let escaped = session_id.replace('\'', "''");
            let batches: Vec<RecordBatch> = table
                .query()
                .nearest_to(query)
                .map_err(|error| anyhow!("lance query: {error}"))?
                .distance_type(DistanceType::Cosine)
                .only_if(format!("session_id = '{escaped}'"))
                .limit(limit)
                .execute()
                .await
                .map_err(|error| anyhow!("lance search: {error}"))?
                .try_collect()
                .await
                .map_err(|error| anyhow!("lance results: {error}"))?;

            let mut results = Vec::new();
            for batch in batches {
                let entry_ids = batch
                    .column_by_name("entry_id")
                    .and_then(|c| c.as_any().downcast_ref::<UInt64Array>())
                    .ok_or_else(|| anyhow!("missing entry_id column"))?;
                let models = batch
                    .column_by_name("model")
                    .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                    .ok_or_else(|| anyhow!("missing model column"))?;
                let distances = batch
                    .column_by_name("_distance")
                    .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
                    .ok_or_else(|| anyhow!("missing _distance column"))?;
                for i in 0..batch.num_rows() {
                    results.push(SimilarEntry {
                        row_id: entry_ids.value(i),
                        entry_id: entry_ids.value(i),
                        score: 1.0 - distances.value(i),
                        model: models.value(i).to_string(),
                    });
                }
            }
            results.sort_by(|a, b| b.score.total_cmp(&a.score));
            results.truncate(limit);
            Ok(results)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lance_store_persists_across_reopen() {
        let dir = tempfile::tempdir().expect("tempdir");
        {
            let store = LanceVectorStore::open(dir.path(), 4).expect("open");
            store
                .upsert("session-a", 1, "test-model", &[1.0, 0.0, 0.0, 0.0])
                .expect("upsert");
        }

        let reopened = LanceVectorStore::open(dir.path(), 4).expect("reopen");
        let results = reopened
            .top_k_similar("session-a", &[1.0, 0.0, 0.0, 0.0], 5)
            .expect("search after reopen");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry_id, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lance_ops_are_safe_inside_multi_thread_tokio_runtime() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = LanceVectorStore::open(dir.path(), 4).expect("open");
        store
            .upsert("session-a", 1, "test-model", &[1.0, 0.0, 0.0, 0.0])
            .expect("upsert inside runtime");
        let results = store
            .top_k_similar("session-a", &[1.0, 0.0, 0.0, 0.0], 1)
            .expect("search inside runtime");
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn lance_ops_are_safe_inside_current_thread_tokio_runtime() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = LanceVectorStore::open(dir.path(), 4).expect("open");
        store
            .upsert("session-a", 1, "test-model", &[1.0, 0.0, 0.0, 0.0])
            .expect("upsert inside runtime");
        let results = store
            .top_k_similar("session-a", &[1.0, 0.0, 0.0, 0.0], 1)
            .expect("search inside runtime");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn open_surfaces_corrupt_table_instead_of_recreating() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A garbage file where the table directory should be produces an open
        // error that is NOT "table missing"; it must surface, not be masked
        // by a create-table attempt.
        std::fs::write(dir.path().join(format!("{TABLE_NAME}.lance")), b"garbage")
            .expect("write garbage");
        let store = LanceVectorStore::open(dir.path(), 4).expect("connect");
        let error = store
            .upsert("session-a", 1, "test-model", &[1.0, 0.0, 0.0, 0.0])
            .expect_err("corrupt table must error");
        assert!(
            error.to_string().contains("opening lance table"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rebuild_from_replaces_table_contents() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = LanceVectorStore::open(dir.path(), 4).expect("open");
        store
            .upsert("stale", 99, "old-model", &[0.5, 0.5, 0.0, 0.0])
            .expect("stale row");

        let source = vec![
            EmbeddingRow {
                session_id: "session-a".to_string(),
                entry_id: 1,
                model: "test-model".to_string(),
                vector: vec![1.0, 0.0, 0.0, 0.0],
            },
            EmbeddingRow {
                session_id: "session-a".to_string(),
                entry_id: 2,
                model: "test-model".to_string(),
                vector: vec![0.0, 1.0, 0.0, 0.0],
            },
        ];
        let rebuilt = store.rebuild_from(source.into_iter()).expect("rebuild");
        assert_eq!(rebuilt, 2);

        let stale = store
            .top_k_similar("stale", &[0.5, 0.5, 0.0, 0.0], 5)
            .expect("stale query");
        assert!(stale.is_empty(), "rebuild must drop pre-existing rows");
        let fresh = store
            .top_k_similar("session-a", &[1.0, 0.0, 0.0, 0.0], 5)
            .expect("fresh query");
        assert_eq!(fresh.len(), 2);
        assert_eq!(fresh[0].entry_id, 1);
    }

    #[test]
    fn lance_store_upserts_and_searches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = LanceVectorStore::open(dir.path(), 4).expect("open");

        store
            .upsert("session-a", 1, "test-model", &[1.0, 0.0, 0.0, 0.0])
            .expect("upsert 1");
        store
            .upsert("session-a", 2, "test-model", &[0.0, 1.0, 0.0, 0.0])
            .expect("upsert 2");
        store
            .upsert("session-b", 3, "test-model", &[1.0, 0.0, 0.0, 0.0])
            .expect("upsert other session");

        let results = store
            .top_k_similar("session-a", &[1.0, 0.0, 0.0, 0.0], 2)
            .expect("search");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].entry_id, 1);
        assert!(results[0].score > 0.99, "score: {}", results[0].score);
        assert!(results[0].score > results[1].score);

        // Upsert replaces, not duplicates
        store
            .upsert("session-a", 1, "test-model", &[0.9, 0.1, 0.0, 0.0])
            .expect("re-upsert");
        let results = store
            .top_k_similar("session-a", &[1.0, 0.0, 0.0, 0.0], 10)
            .expect("search after re-upsert");
        assert_eq!(results.len(), 2, "merge-insert must not duplicate rows");
    }
}
