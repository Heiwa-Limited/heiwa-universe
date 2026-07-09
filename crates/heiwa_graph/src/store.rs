//! SQLite-backed graph operations.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use heiwa_protocol::{
    CalendarProposal, GraphEdge, GraphEntityKind, GraphNode, ProposalStatus, Sensitivity,
    SourceSpan, SOVEREIGN_GRAPH_SCHEMA_VERSION,
};
use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;
use uuid::Uuid;

use crate::schema::{SCHEMA_SQL, SCHEMA_VERSION};

#[derive(Debug, Error)]
pub enum GraphStoreError {
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

pub struct GraphStore {
    conn: Connection,
}

impl GraphStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create graph dir {}", parent.display()))?;
        }
        let conn = Connection::open(path.as_ref())
            .with_context(|| format!("open graph db {}", path.as_ref().display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA_SQL)?;
        let current: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM graph_meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match current.as_deref() {
            None => {
                self.conn.execute(
                    "INSERT INTO graph_meta(key, value) VALUES ('schema_version', ?1)",
                    params![SCHEMA_VERSION],
                )?;
            }
            Some(v) if v == SCHEMA_VERSION || v == SOVEREIGN_GRAPH_SCHEMA_VERSION => {}
            Some(v) => {
                anyhow::bail!(
                    "unsupported sovereign graph schema version {v}; expected {SCHEMA_VERSION}"
                );
            }
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<String> {
        let v: String = self.conn.query_row(
            "SELECT value FROM graph_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )?;
        Ok(v)
    }

    pub fn upsert_node(&self, node: &GraphNode) -> Result<()> {
        let props = serde_json::to_string(&node.properties)?;
        self.conn.execute(
            r#"
            INSERT INTO nodes(
                id, kind, title, sensitivity, created_at_unix, updated_at_unix,
                source_system, external_id, properties_json
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
            ON CONFLICT(id) DO UPDATE SET
                kind=excluded.kind,
                title=excluded.title,
                sensitivity=excluded.sensitivity,
                updated_at_unix=excluded.updated_at_unix,
                source_system=excluded.source_system,
                external_id=excluded.external_id,
                properties_json=excluded.properties_json
            "#,
            params![
                node.id,
                node.kind.as_str(),
                node.title,
                sensitivity_str(node.sensitivity),
                node.created_at_unix,
                node.updated_at_unix,
                node.source_system,
                node.external_id,
                props,
            ],
        )?;
        if let Some(title) = &node.title {
            let _ = self.conn.execute(
                "INSERT INTO nodes_fts(rowid, title, body) VALUES (
                    (SELECT rowid FROM nodes WHERE id = ?1), ?2, ?3
                 )",
                params![
                    node.id,
                    title,
                    node.properties
                        .get("body")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                ],
            );
        }
        Ok(())
    }

    pub fn get_node(&self, id: &str) -> Result<Option<GraphNode>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, kind, title, sensitivity, created_at_unix, updated_at_unix,
                   source_system, external_id, properties_json
            FROM nodes WHERE id = ?1
            "#,
        )?;
        let node = stmt
            .query_row(params![id], |row| {
                Ok(GraphNode {
                    id: row.get(0)?,
                    kind: parse_kind(&row.get::<_, String>(1)?),
                    title: row.get(2)?,
                    sensitivity: parse_sensitivity(&row.get::<_, String>(3)?),
                    created_at_unix: row.get(4)?,
                    updated_at_unix: row.get(5)?,
                    source_system: row.get(6)?,
                    external_id: row.get(7)?,
                    properties: serde_json::from_str(&row.get::<_, String>(8)?)
                        .unwrap_or(serde_json::json!({})),
                })
            })
            .optional()?;
        Ok(node)
    }

    pub fn insert_edge(&self, edge: &GraphEdge) -> Result<()> {
        let props = serde_json::to_string(&edge.properties)?;
        self.conn.execute(
            r#"
            INSERT INTO edges(id, from_id, to_id, kind, created_at_unix, properties_json)
            VALUES (?1,?2,?3,?4,?5,?6)
            "#,
            params![
                edge.id,
                edge.from_id,
                edge.to_id,
                edge.kind,
                edge.created_at_unix,
                props
            ],
        )?;
        Ok(())
    }

    pub fn attach_source_span(&self, node_id: &str, span: &SourceSpan) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            r#"
            INSERT INTO source_spans(id, node_id, kind, locator, start_off, end_off)
            VALUES (?1,?2,?3,?4,?5,?6)
            "#,
            params![id, node_id, span.kind, span.locator, span.start, span.end],
        )?;
        Ok(id)
    }

    pub fn upsert_calendar_proposal(&self, proposal: &CalendarProposal) -> Result<()> {
        let now = now_unix();
        self.conn.execute(
            r#"
            INSERT INTO calendar_proposals(
                id, title, starts_at_unix, ends_at_unix, confidence,
                attendees_json, notes, status, sources_json, created_at_unix, updated_at_unix
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
            ON CONFLICT(id) DO UPDATE SET
                title=excluded.title,
                starts_at_unix=excluded.starts_at_unix,
                ends_at_unix=excluded.ends_at_unix,
                confidence=excluded.confidence,
                attendees_json=excluded.attendees_json,
                notes=excluded.notes,
                status=excluded.status,
                sources_json=excluded.sources_json,
                updated_at_unix=excluded.updated_at_unix
            "#,
            params![
                proposal.id,
                proposal.title,
                proposal.starts_at_unix,
                proposal.ends_at_unix,
                proposal.confidence,
                serde_json::to_string(&proposal.attendees)?,
                proposal.notes,
                proposal_status_str(proposal.status),
                serde_json::to_string(&proposal.sources)?,
                now,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn count_nodes(&self) -> Result<i64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
        Ok(n)
    }

    pub fn doctor_summary(&self) -> Result<String> {
        let version = self.schema_version()?;
        let nodes = self.count_nodes()?;
        let edges: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))?;
        let proposals: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM calendar_proposals",
            [],
            |r| r.get(0),
        )?;
        Ok(format!(
            "sovereign_graph schema={version} nodes={nodes} edges={edges} calendar_proposals={proposals}"
        ))
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn sensitivity_str(s: Sensitivity) -> &'static str {
    match s {
        Sensitivity::Public => "public",
        Sensitivity::Internal => "internal",
        Sensitivity::Private => "private",
        Sensitivity::Secret => "secret",
    }
}

fn parse_sensitivity(s: &str) -> Sensitivity {
    match s {
        "public" => Sensitivity::Public,
        "internal" => Sensitivity::Internal,
        "secret" => Sensitivity::Secret,
        _ => Sensitivity::Private,
    }
}

fn parse_kind(s: &str) -> GraphEntityKind {
    match s {
        "person" => GraphEntityKind::Person,
        "account" => GraphEntityKind::Account,
        "thread" => GraphEntityKind::Thread,
        "message" => GraphEntityKind::Message,
        "event" => GraphEntityKind::Event,
        "note" => GraphEntityKind::Note,
        "file" => GraphEntityKind::File,
        "web_doc" => GraphEntityKind::WebDoc,
        "task" => GraphEntityKind::Task,
        "device" => GraphEntityKind::Device,
        "project" => GraphEntityKind::Project,
        "receipt" => GraphEntityKind::Receipt,
        "memory" => GraphEntityKind::Memory,
        _ => GraphEntityKind::Note,
    }
}

fn proposal_status_str(s: ProposalStatus) -> &'static str {
    match s {
        ProposalStatus::Draft => "draft",
        ProposalStatus::PendingApproval => "pending_approval",
        ProposalStatus::Approved => "approved",
        ProposalStatus::Rejected => "rejected",
        ProposalStatus::Written => "written",
    }
}

/// Helper to build a node with fresh timestamps.
pub fn new_node(kind: GraphEntityKind, title: impl Into<String>) -> GraphNode {
    let ts = now_unix();
    GraphNode {
        id: Uuid::new_v4().to_string(),
        kind,
        title: Some(title.into()),
        sensitivity: Sensitivity::Private,
        created_at_unix: ts,
        updated_at_unix: ts,
        source_system: None,
        external_id: None,
        properties: serde_json::json!({}),
    }
}
