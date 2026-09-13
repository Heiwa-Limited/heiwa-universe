use std::collections::BTreeMap;

use heiwa_work::{
    agent_view, home_view, surfaces, view_for, work_view, DeltaApplyOutcome, ProjectionEpoch,
    ResyncReason, WorkSessionDeltaV1, WorkSessionSnapshotV1, SURFACES,
};
use serde_json::json;

fn snapshot() -> WorkSessionSnapshotV1 {
    let mut collections = BTreeMap::new();
    let mut rows = BTreeMap::new();
    rows.insert(
        "work-1".to_string(),
        json!({"intent": "ship it", "status": "open"}),
    );
    collections.insert("work".to_string(), rows);
    let mut runs = BTreeMap::new();
    runs.insert(
        "run-2".to_string(),
        json!({"worker_state": "stale", "supervision": {"process": "alive"}}),
    );
    collections.insert("runs".to_string(), runs);
    let mut artifacts = BTreeMap::new();
    artifacts.insert("artifact-1".to_string(), json!({}));
    collections.insert("artifacts".to_string(), artifacts);
    let mut truncated = BTreeMap::new();
    truncated.insert("runs".to_string(), 3);
    truncated.insert("artifacts".to_string(), 7);

    WorkSessionSnapshotV1 {
        work_id: "work-1".to_string(),
        work_revision: 4,
        projection_epoch: ProjectionEpoch::from_seed("fold-1"),
        projection_revision: 9,
        operator_cursor: Some("cursor-9".to_string()),
        source_watermarks: BTreeMap::new(),
        collections,
        truncated_collections: truncated,
    }
}

#[test]
fn every_surface_reports_the_identity_of_the_one_snapshot_it_came_from() {
    let snapshot = snapshot();
    let views = surfaces(&snapshot);
    assert_eq!(
        views
            .iter()
            .map(|view| view.surface.as_str())
            .collect::<Vec<_>>(),
        SURFACES
    );
    for view in &views {
        assert_eq!(view.identity.work_id, snapshot.work_id);
        assert_eq!(view.identity.work_revision, snapshot.work_revision);
        assert_eq!(view.identity.projection_epoch, snapshot.projection_epoch);
        assert_eq!(
            view.identity.projection_revision,
            snapshot.projection_revision
        );
        assert_eq!(view.identity.operator_cursor, snapshot.operator_cursor);
        assert_eq!(view_for(&snapshot, &view.surface).as_ref(), Some(view));
    }
    assert!(view_for(&snapshot, "settings").is_none());
}

#[test]
fn surfaces_that_show_runs_show_the_same_rows_and_the_same_bound() {
    let snapshot = snapshot();
    let (home, work, agent) = (
        home_view(&snapshot),
        work_view(&snapshot),
        agent_view(&snapshot),
    );
    for view in [&home, &work, &agent] {
        assert_eq!(
            view.collections.get("runs"),
            snapshot.collections.get("runs")
        );
        // A bound is carried, never rendered as completeness.
        assert_eq!(view.truncated_collections.get("runs"), Some(&3));
    }
    // A view that omits a collection omits its bound too, and never invents rows.
    assert!(!home.collections.contains_key("artifacts"));
    assert!(!home.truncated_collections.contains_key("artifacts"));
    assert_eq!(work.truncated_collections.get("artifacts"), Some(&7));
}

#[test]
fn no_surface_accepts_an_update_from_another_work_fold_or_revision() {
    let snapshot = snapshot();
    let delta = |work: &str, epoch: &str, base: u64, next: u64| WorkSessionDeltaV1 {
        work_id: work.to_string(),
        projection_epoch: ProjectionEpoch::from_seed(epoch),
        base_projection_revision: base,
        projection_revision: next,
        operator_cursor: Some("cursor-10".to_string()),
        upserts: BTreeMap::new(),
        removals: BTreeMap::new(),
    };
    for view in surfaces(&snapshot) {
        let client = view.identity.client();
        assert_eq!(
            client.accept(&delta("work-1", "fold-1", 9, 10)),
            DeltaApplyOutcome::Applied {
                projection_revision: 10
            }
        );
        assert_eq!(
            client.accept(&delta("work-2", "fold-1", 9, 10)),
            DeltaApplyOutcome::ResyncRequired {
                reason: ResyncReason::WorkChanged
            }
        );
        assert_eq!(
            client.accept(&delta("work-1", "fold-2", 9, 10)),
            DeltaApplyOutcome::ResyncRequired {
                reason: ResyncReason::EpochChanged
            }
        );
        // A stale frame (already applied) and a gap are both refused.
        assert_eq!(
            client.accept(&delta("work-1", "fold-1", 8, 9)),
            DeltaApplyOutcome::ResyncRequired {
                reason: ResyncReason::RevisionGap
            }
        );
        assert_eq!(
            client.accept(&delta("work-1", "fold-1", 10, 11)),
            DeltaApplyOutcome::ResyncRequired {
                reason: ResyncReason::RevisionGap
            }
        );
    }
}
