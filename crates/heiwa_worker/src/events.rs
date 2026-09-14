//! Worker and pane facts as operator-domain events.
//!
//! These join the one operator stream rather than a second store, so replaying
//! one Work produces the whole of it — including who ran, where, and how it
//! ended. The actor is the worker, never the human: the spec forbids a worker
//! claiming the human actor.

use heiwa_evidence::{
    OperatorActor, OperatorEvent, OperatorEventType, OperatorRisk, OperatorSensitivity,
    OPERATOR_EVENT_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

use crate::model::{ObservedProcess, PaneIdentity, WorkerIdentity, SCHEMA_VERSION};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerLaunchedPayload {
    pub worker_id: String,
    pub provider: String,
    pub provider_session_ref: Option<String>,
    pub executable_path: String,
    pub executable_sha256: String,
    pub cwd: String,
    pub repo_root: String,
    pub branch: String,
    pub base_commit: String,
    pub lease_id: String,
    pub installation_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerHeartbeatPayload {
    pub worker_id: String,
    pub pid: u32,
    /// Platform start identity of the process holding `pid`, compared only
    /// for equality. It is what lets recovery tell the recorded process from
    /// a later one that reused the pid. Absent on records that predate it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_start_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerExitedPayload {
    pub worker_id: String,
    /// `None` when the process was signalled or never started.
    pub exit_code: Option<i32>,
    /// Set when the worker failed rather than completing.
    pub failure_code: Option<String>,
}

/// Restart recovery's record that a run lost its supervisor.
///
/// Two separate facts: supervision was lost (`reason`), and what recovery saw
/// of the process when it looked (`process`). Neither is an exit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerStalePayload {
    pub worker_id: String,
    pub reason: String,
    pub process: ObservedProcess,
    /// The pid recovery looked for, when one was ever recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}

/// The only reason recovery marks a run stale today: nothing supervises it.
pub const OWNER_LOST: &str = "owner_lost";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneOpenedPayload {
    pub pane_id: String,
    pub worker_id: String,
    pub cwd: String,
    pub repo_root: String,
    pub branch: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneClosedPayload {
    pub pane_id: String,
    pub worker_id: String,
    /// Bounded, truncated tail of what the pane showed. Never the full log.
    pub tail: Vec<String>,
    /// Lines dropped before `tail` begins, so the reader knows it is a tail.
    pub dropped_lines: usize,
}

macro_rules! from_event {
    ($ty:ty, $variant:expr) => {
        impl $ty {
            pub fn from_event(event: &OperatorEvent) -> Option<Self> {
                if event.event_type != $variant {
                    return None;
                }
                serde_json::from_value(event.payload.clone()).ok()
            }
        }
    };
}

from_event!(WorkerLaunchedPayload, OperatorEventType::WorkerLaunched);
from_event!(WorkerHeartbeatPayload, OperatorEventType::WorkerHeartbeat);
from_event!(WorkerExitedPayload, OperatorEventType::WorkerExited);
from_event!(WorkerStalePayload, OperatorEventType::WorkerStale);
from_event!(PaneOpenedPayload, OperatorEventType::PaneOpened);
from_event!(PaneClosedPayload, OperatorEventType::PaneClosed);

/// The address of one recorded run.
///
/// Recovery has no live [`WorkerIdentity`] — only what the journal recorded —
/// so a marker is addressed by the ids that make it land on exactly one run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunRef<'a> {
    pub work_id: &'a str,
    pub thread_id: &'a str,
    pub run_id: &'a str,
    pub worker_id: &'a str,
}

impl<'a> RunRef<'a> {
    pub fn of(identity: &'a WorkerIdentity, run_id: &'a str) -> Self {
        Self {
            work_id: &identity.work_id,
            thread_id: &identity.thread_id,
            run_id,
            worker_id: &identity.worker_id,
        }
    }
}

/// One worker-authored event, scoped to its Work and thread.
///
/// `run_id` identifies one process invocation, while `worker_id` identifies
/// the prepared worker that owns the lease. Both are readable without
/// confusing execution history with worker ownership.
struct WorkerScope<'a> {
    work_id: &'a str,
    thread_id: &'a str,
    run_id: &'a str,
    worker_id: &'a str,
}

fn worker_scoped(
    scope: WorkerScope<'_>,
    event_type: OperatorEventType,
    occurred_at: &str,
    payload: serde_json::Value,
    new_event_id: impl FnOnce() -> String,
) -> OperatorEvent {
    OperatorEvent {
        schema_version: OPERATOR_EVENT_SCHEMA_VERSION,
        event_id: new_event_id(),
        thread_id: scope.thread_id.to_string(),
        turn_id: None,
        run_id: Some(scope.run_id.to_string()),
        call_id: None,
        work_id: Some(scope.work_id.to_string()),
        event_type,
        occurred_at: occurred_at.to_string(),
        actor: OperatorActor {
            kind: "worker".to_string(),
            id: scope.worker_id.to_string(),
        },
        risk_class: OperatorRisk::Low,
        sensitivity: OperatorSensitivity::LocalPrivate,
        parent_event_id: None,
        correlation_id: None,
        source_refs: Vec::new(),
        evidence_refs: Vec::new(),
        payload,
    }
}

pub fn worker_launched_event(
    identity: &WorkerIdentity,
    run_id: &str,
    occurred_at: &str,
    new_event_id: impl FnOnce() -> String,
) -> OperatorEvent {
    debug_assert_eq!(identity.schema_version, SCHEMA_VERSION);
    let payload = serde_json::to_value(WorkerLaunchedPayload {
        worker_id: identity.worker_id.clone(),
        provider: identity.provider.clone(),
        provider_session_ref: identity.provider_session_ref.clone(),
        executable_path: identity.executable_path.clone(),
        executable_sha256: identity.executable_sha256.clone(),
        cwd: identity.cwd.clone(),
        repo_root: identity.repo_root.clone(),
        branch: identity.branch.clone(),
        base_commit: identity.base_commit.clone(),
        lease_id: identity.lease_id.clone(),
        installation_id: identity.installation_id.clone(),
    })
    .expect("worker launch payload is plain data");
    worker_scoped(
        WorkerScope {
            work_id: &identity.work_id,
            thread_id: &identity.thread_id,
            run_id,
            worker_id: &identity.worker_id,
        },
        OperatorEventType::WorkerLaunched,
        occurred_at,
        payload,
        new_event_id,
    )
}

pub fn worker_heartbeat_event(
    identity: &WorkerIdentity,
    run_id: &str,
    pid: u32,
    process_start_id: Option<&str>,
    occurred_at: &str,
    new_event_id: impl FnOnce() -> String,
) -> OperatorEvent {
    let payload = serde_json::to_value(WorkerHeartbeatPayload {
        worker_id: identity.worker_id.clone(),
        pid,
        process_start_id: process_start_id.map(str::to_string),
    })
    .expect("worker heartbeat payload is plain data");
    worker_scoped(
        WorkerScope {
            work_id: &identity.work_id,
            thread_id: &identity.thread_id,
            run_id,
            worker_id: &identity.worker_id,
        },
        OperatorEventType::WorkerHeartbeat,
        occurred_at,
        payload,
        new_event_id,
    )
}

pub fn worker_exited_event(
    identity: &WorkerIdentity,
    run_id: &str,
    exit_code: Option<i32>,
    failure_code: Option<String>,
    occurred_at: &str,
    new_event_id: impl FnOnce() -> String,
) -> OperatorEvent {
    let payload = serde_json::to_value(WorkerExitedPayload {
        worker_id: identity.worker_id.clone(),
        exit_code,
        failure_code,
    })
    .expect("worker exit payload is plain data");
    worker_scoped(
        WorkerScope {
            work_id: &identity.work_id,
            thread_id: &identity.thread_id,
            run_id,
            worker_id: &identity.worker_id,
        },
        OperatorEventType::WorkerExited,
        occurred_at,
        payload,
        new_event_id,
    )
}

/// Mark one run as no longer supervised, recording what recovery observed of
/// its process. The actor is the runtime, not the worker: the worker did not
/// say this about itself.
pub fn worker_stale_event(
    run: RunRef<'_>,
    process: ObservedProcess,
    pid: Option<u32>,
    occurred_at: &str,
    new_event_id: impl FnOnce() -> String,
) -> OperatorEvent {
    let payload = serde_json::to_value(WorkerStalePayload {
        worker_id: run.worker_id.to_string(),
        reason: OWNER_LOST.to_string(),
        process,
        pid,
    })
    .expect("worker stale payload is plain data");
    let mut event = worker_scoped(
        WorkerScope {
            work_id: run.work_id,
            thread_id: run.thread_id,
            run_id: run.run_id,
            worker_id: run.worker_id,
        },
        OperatorEventType::WorkerStale,
        occurred_at,
        payload,
        new_event_id,
    );
    event.actor = OperatorActor {
        kind: "runtime".to_string(),
        id: "worker-recovery".to_string(),
    };
    event
}

pub fn pane_opened_event(
    pane: &PaneIdentity,
    run_id: &str,
    thread_id: &str,
    occurred_at: &str,
    new_event_id: impl FnOnce() -> String,
) -> OperatorEvent {
    debug_assert_eq!(pane.schema_version, SCHEMA_VERSION);
    let payload = serde_json::to_value(PaneOpenedPayload {
        pane_id: pane.pane_id.clone(),
        worker_id: pane.worker_id.clone(),
        cwd: pane.cwd.clone(),
        repo_root: pane.repo_root.clone(),
        branch: pane.branch.clone(),
    })
    .expect("pane open payload is plain data");
    worker_scoped(
        WorkerScope {
            work_id: &pane.work_id,
            thread_id,
            run_id,
            worker_id: &pane.worker_id,
        },
        OperatorEventType::PaneOpened,
        occurred_at,
        payload,
        new_event_id,
    )
}

pub fn pane_closed_event(
    pane: &PaneIdentity,
    run_id: &str,
    thread_id: &str,
    tail: Vec<String>,
    dropped_lines: usize,
    occurred_at: &str,
    new_event_id: impl FnOnce() -> String,
) -> OperatorEvent {
    let payload = serde_json::to_value(PaneClosedPayload {
        pane_id: pane.pane_id.clone(),
        worker_id: pane.worker_id.clone(),
        tail,
        dropped_lines,
    })
    .expect("pane close payload is plain data");
    worker_scoped(
        WorkerScope {
            work_id: &pane.work_id,
            thread_id,
            run_id,
            worker_id: &pane.worker_id,
        },
        OperatorEventType::PaneClosed,
        occurred_at,
        payload,
        new_event_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> WorkerIdentity {
        WorkerIdentity {
            schema_version: SCHEMA_VERSION,
            worker_id: "worker-1".into(),
            work_id: "work-1".into(),
            thread_id: "thread-1".into(),
            provider: "claude".into(),
            provider_session_ref: None,
            executable_path: "/usr/local/bin/claude".into(),
            executable_sha256: "a".repeat(64),
            cwd: "/tmp/worktrees/work-1".into(),
            repo_root: "/tmp/repo".into(),
            branch: "heiwa/work-1".into(),
            base_commit: "b".repeat(40),
            lease_id: "lease-1".into(),
            installation_id: "install-1".into(),
            started_at: "2026-08-26T00:00:00Z".into(),
        }
    }

    #[test]
    fn launch_event_carries_work_scope_on_the_envelope() {
        let event =
            worker_launched_event(&identity(), "run-1", "2026-08-26T00:00:00Z", || "e1".into());
        assert_eq!(event.work_id.as_deref(), Some("work-1"));
        assert_eq!(event.run_id.as_deref(), Some("run-1"));
        assert_eq!(event.thread_id, "thread-1");
        assert_eq!(event.event_type, OperatorEventType::WorkerLaunched);
        assert_eq!(
            WorkerLaunchedPayload::from_event(&event)
                .expect("payload")
                .worker_id,
            "worker-1"
        );
    }

    #[test]
    fn the_actor_is_the_worker_not_the_human() {
        let event =
            worker_launched_event(&identity(), "run-1", "2026-08-26T00:00:00Z", || "e1".into());
        assert_eq!(event.actor.kind, "worker");
        assert_eq!(event.actor.id, "worker-1");
    }

    #[test]
    fn payload_readers_refuse_the_wrong_event_type() {
        let event =
            worker_launched_event(&identity(), "run-1", "2026-08-26T00:00:00Z", || "e1".into());
        assert!(WorkerExitedPayload::from_event(&event).is_none());
    }
}
