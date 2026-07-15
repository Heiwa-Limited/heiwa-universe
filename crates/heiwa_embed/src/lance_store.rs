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

use crate::SimilarEntry;

const TABLE_NAME: &str = "transcript_embeddings";

pub struct LanceVectorStore {
    conn: Connection,
    runtime: tokio::runtime::Runtime,
    dim: i32,
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
        let conn = runtime.block_on(async { lancedb::connect(&uri).execute().await })?;
        Ok(Self {
            conn,
            runtime,
            dim: dim as i32,
        })
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
            Err(_) => {
                let empty: Box<dyn RecordBatchReader + Send> = Box::new(
                    RecordBatchIterator::new(std::iter::empty::<Result<RecordBatch, ArrowError>>(), self.schema()),
                );
                self.conn
                    .create_table(TABLE_NAME, empty)
                    .execute()
                    .await
                    .map_err(|error| anyhow!("creating lance table: {error}"))
            }
        }
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
        self.runtime.block_on(async {
            let table = self.table().await?;
            let mut merge = table.merge_insert(&["session_id", "entry_id"]);
            merge.when_matched_update_all(None).when_not_matched_insert_all();
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
        self.runtime.block_on(async {
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
