//! Home, Work, and Agent as pure selections over one Work-session snapshot.
//!
//! The surfaces must never disagree about what is running, blocked, approved,
//! changed, or complete. That is enforced here by construction: each view
//! takes `&WorkSessionSnapshotV1` and copies its identity from it, so there is
//! no second place a revision, epoch, cursor, or bound could come from. A view
//! selects and reshapes; it never re-folds.
//!
//! Agreement holds within one snapshot. Two snapshots built separately carry
//! different epochs by design, so callers serve the three views of one request
//! from one snapshot ([`surfaces`]) and track updates with the same
//! [`ClientProjection`] guard every surface derives.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::snapshot::{ClientProjection, CollectionRows, ProjectionEpoch, WorkSessionSnapshotV1};

/// The identity every surface must agree on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceIdentity {
    pub work_id: String,
    pub work_revision: u64,
    pub projection_epoch: ProjectionEpoch,
    pub projection_revision: u64,
    pub operator_cursor: Option<String>,
}

impl SurfaceIdentity {
    fn of(snapshot: &WorkSessionSnapshotV1) -> Self {
        Self {
            work_id: snapshot.work_id.clone(),
            work_revision: snapshot.work_revision,
            projection_epoch: snapshot.projection_epoch.clone(),
            projection_revision: snapshot.projection_revision,
            operator_cursor: snapshot.operator_cursor.clone(),
        }
    }

    /// The delta guard a consumer of this surface tracks. Deltas for another
    /// Work, another fold, or a revision other than the next one are refused
    /// rather than silently applied.
    pub fn client(&self) -> ClientProjection {
        ClientProjection {
            work_id: self.work_id.clone(),
            epoch: self.projection_epoch.clone(),
            projection_revision: self.projection_revision,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceView {
    pub surface: String,
    pub identity: SurfaceIdentity,
    pub collections: BTreeMap<String, CollectionRows>,
    /// Rows omitted at the snapshot's bound for each selected collection,
    /// carried verbatim so a surface can never render a bound as completeness.
    pub truncated_collections: BTreeMap<String, usize>,
}

pub const SURFACES: [&str; 3] = ["home", "work", "agent"];

fn select(snapshot: &WorkSessionSnapshotV1, surface: &str, names: &[&str]) -> SurfaceView {
    let mut collections = BTreeMap::new();
    let mut truncated = BTreeMap::new();
    for name in names {
        if let Some(rows) = snapshot.collections.get(*name) {
            collections.insert((*name).to_string(), rows.clone());
        }
        if let Some(count) = snapshot.truncated_collections.get(*name) {
            truncated.insert((*name).to_string(), *count);
        }
    }
    SurfaceView {
        surface: surface.to_string(),
        identity: SurfaceIdentity::of(snapshot),
        collections,
        truncated_collections: truncated,
    }
}

/// Home: what needs the user, what is working, what recently completed.
pub fn home_view(snapshot: &WorkSessionSnapshotV1) -> SurfaceView {
    select(
        snapshot,
        "home",
        &["work", "blockers", "approvals", "runs", "receipts"],
    )
}

/// Work: objective, conversation, workspace, and the evidence it produced.
pub fn work_view(snapshot: &WorkSessionSnapshotV1) -> SurfaceView {
    select(
        snapshot,
        "work",
        &[
            "work",
            "threads",
            "workspace",
            "runs",
            "actions",
            "artifacts",
            "tests",
            "receipts",
            "blockers",
        ],
    )
}

/// Agent: runs, panes, worktrees, and the actions they took.
pub fn agent_view(snapshot: &WorkSessionSnapshotV1) -> SurfaceView {
    select(
        snapshot,
        "agent",
        &["work", "runs", "workspace", "actions", "approvals"],
    )
}

/// One named surface, or `None` for a name this build does not serve.
pub fn view_for(snapshot: &WorkSessionSnapshotV1, surface: &str) -> Option<SurfaceView> {
    match surface {
        "home" => Some(home_view(snapshot)),
        "work" => Some(work_view(snapshot)),
        "agent" => Some(agent_view(snapshot)),
        _ => None,
    }
}

/// Every surface, from the one snapshot a request built.
pub fn surfaces(snapshot: &WorkSessionSnapshotV1) -> Vec<SurfaceView> {
    vec![
        home_view(snapshot),
        work_view(snapshot),
        agent_view(snapshot),
    ]
}
