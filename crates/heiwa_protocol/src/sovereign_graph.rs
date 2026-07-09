//! Sovereign Graph v1 — entity model for the on-device digital database.
//!
//! This is the *schema of meaning* for consolidating mail, calendar, messages,
//! files, web crawls, and receipts without a third-party cloud dossier.
//! Persistence lives in `heiwa_graph`; this module is shared protocol types.

use serde::{Deserialize, Serialize};

/// Schema version string stored in `graph_meta`.
pub const SOVEREIGN_GRAPH_SCHEMA_VERSION: &str = "1";

/// Stable entity kinds in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphEntityKind {
    Person,
    Account,
    Thread,
    Message,
    Event,
    Note,
    File,
    WebDoc,
    Task,
    Device,
    Project,
    Receipt,
    Memory,
}

impl GraphEntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Account => "account",
            Self::Thread => "thread",
            Self::Message => "message",
            Self::Event => "event",
            Self::Note => "note",
            Self::File => "file",
            Self::WebDoc => "web_doc",
            Self::Task => "task",
            Self::Device => "device",
            Self::Project => "project",
            Self::Receipt => "receipt",
            Self::Memory => "memory",
        }
    }

    pub const ALL: &'static [Self] = &[
        Self::Person,
        Self::Account,
        Self::Thread,
        Self::Message,
        Self::Event,
        Self::Note,
        Self::File,
        Self::WebDoc,
        Self::Task,
        Self::Device,
        Self::Project,
        Self::Receipt,
        Self::Memory,
    ];
}

/// Sensitivity for retention / display / export redaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Internal,
    Private,
    Secret,
}

/// Edge labels between entities (open set with recommended values).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GraphEdgeKind(pub String);

impl GraphEdgeKind {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub const PARTICIPANT: &'static str = "participant";
    pub const MEMBER_OF: &'static str = "member_of";
    pub const REPLY_TO: &'static str = "reply_to";
    pub const ABOUT: &'static str = "about";
    pub const DERIVED_FROM: &'static str = "derived_from";
    pub const ATTACHMENT: &'static str = "attachment";
    pub const SCHEDULED_AS: &'static str = "scheduled_as";
    pub const CITES: &'static str = "cites";
    pub const OWNED_BY: &'static str = "owned_by";
}

/// Source span for evidence (HEIWA.md target).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub kind: String,
    pub locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<i64>,
}

impl SourceSpan {
    pub fn message_id(id: impl Into<String>) -> Self {
        Self {
            kind: "message_id".into(),
            locator: id.into(),
            start: None,
            end: None,
        }
    }

    pub fn event_id(id: impl Into<String>) -> Self {
        Self {
            kind: "event_id".into(),
            locator: id.into(),
            start: None,
            end: None,
        }
    }

    pub fn file_lines(path: impl Into<String>, start: i64, end: i64) -> Self {
        Self {
            kind: "file".into(),
            locator: path.into(),
            start: Some(start),
            end: Some(end),
        }
    }

    pub fn url(url: impl Into<String>) -> Self {
        Self {
            kind: "url".into(),
            locator: url.into(),
            start: None,
            end: None,
        }
    }
}

/// Calendar proposal staged for approval (never silent write).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarProposal {
    pub id: String,
    pub title: String,
    pub starts_at_unix: i64,
    pub ends_at_unix: i64,
    pub confidence: f32,
    pub sources: Vec<SourceSpan>,
    pub attendees: Vec<String>,
    pub notes: Option<String>,
    pub status: ProposalStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Draft,
    PendingApproval,
    Approved,
    Rejected,
    Written,
}

/// Generic graph node payload for transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: GraphEntityKind,
    pub title: Option<String>,
    pub sensitivity: Sensitivity,
    pub created_at_unix: i64,
    pub updated_at_unix: i64,
    /// Connector or system that produced the row (e.g. `google_calendar`).
    pub source_system: Option<String>,
    /// External stable id at source (message id, event id, …).
    pub external_id: Option<String>,
    #[serde(default)]
    pub properties: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    pub kind: String,
    pub created_at_unix: i64,
    #[serde(default)]
    pub properties: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_kind_roundtrip_strings() {
        for k in GraphEntityKind::ALL {
            assert!(!k.as_str().is_empty());
        }
        assert_eq!(SOVEREIGN_GRAPH_SCHEMA_VERSION, "1");
    }
}
