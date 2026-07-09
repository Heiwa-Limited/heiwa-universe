//! SQL for Sovereign Graph schema v1.

use heiwa_protocol::SOVEREIGN_GRAPH_SCHEMA_VERSION;

pub const SCHEMA_VERSION: &str = SOVEREIGN_GRAPH_SCHEMA_VERSION;

/// Full DDL applied on open (idempotent).
pub const SCHEMA_SQL: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS graph_meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS nodes (
    id              TEXT PRIMARY KEY NOT NULL,
    kind            TEXT NOT NULL,
    title           TEXT,
    sensitivity     TEXT NOT NULL DEFAULT 'private',
    created_at_unix INTEGER NOT NULL,
    updated_at_unix INTEGER NOT NULL,
    source_system   TEXT,
    external_id     TEXT,
    properties_json TEXT NOT NULL DEFAULT '{}',
    UNIQUE(source_system, external_id)
);

CREATE INDEX IF NOT EXISTS idx_nodes_kind ON nodes(kind);
CREATE INDEX IF NOT EXISTS idx_nodes_updated ON nodes(updated_at_unix);
CREATE INDEX IF NOT EXISTS idx_nodes_external ON nodes(source_system, external_id);

CREATE TABLE IF NOT EXISTS edges (
    id              TEXT PRIMARY KEY NOT NULL,
    from_id         TEXT NOT NULL,
    to_id           TEXT NOT NULL,
    kind            TEXT NOT NULL,
    created_at_unix INTEGER NOT NULL,
    properties_json TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY(from_id) REFERENCES nodes(id) ON DELETE CASCADE,
    FOREIGN KEY(to_id) REFERENCES nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);
CREATE INDEX IF NOT EXISTS idx_edges_kind ON edges(kind);

CREATE TABLE IF NOT EXISTS blobs (
    sha256      TEXT PRIMARY KEY NOT NULL,
    byte_len    INTEGER NOT NULL,
    mime        TEXT,
    created_at_unix INTEGER NOT NULL,
    path_rel    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS node_blobs (
    node_id TEXT NOT NULL,
    sha256  TEXT NOT NULL,
    role    TEXT NOT NULL DEFAULT 'body',
    PRIMARY KEY(node_id, sha256, role),
    FOREIGN KEY(node_id) REFERENCES nodes(id) ON DELETE CASCADE,
    FOREIGN KEY(sha256) REFERENCES blobs(sha256) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS source_spans (
    id         TEXT PRIMARY KEY NOT NULL,
    node_id    TEXT NOT NULL,
    kind       TEXT NOT NULL,
    locator    TEXT NOT NULL,
    start_off  INTEGER,
    end_off    INTEGER,
    FOREIGN KEY(node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_spans_node ON source_spans(node_id);

CREATE TABLE IF NOT EXISTS calendar_proposals (
    id              TEXT PRIMARY KEY NOT NULL,
    title           TEXT NOT NULL,
    starts_at_unix  INTEGER NOT NULL,
    ends_at_unix    INTEGER NOT NULL,
    confidence      REAL NOT NULL,
    attendees_json  TEXT NOT NULL DEFAULT '[]',
    notes           TEXT,
    status          TEXT NOT NULL,
    sources_json    TEXT NOT NULL DEFAULT '[]',
    created_at_unix INTEGER NOT NULL,
    updated_at_unix INTEGER NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts USING fts5(
    title,
    body,
    tokenize = 'porter'
);
"#;
