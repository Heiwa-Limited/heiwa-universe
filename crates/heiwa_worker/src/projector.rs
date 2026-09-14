//! Pure fold from the operator stream to the `runs` read model.
//!
//! Append order is authority. A row is keyed by per-invocation `run_id` and only ever
//! created by a `worker_launched` envelope scoped to the requested Work — a
//! pane alone never mints one, because the spec forbids treating a terminal
//! pane as a verified worker merely because it exists.

use std::collections::BTreeMap;

use heiwa_evidence::{OperatorEvent, OperatorEventType};
use serde::{Deserialize, Serialize};

use crate::events::{
    PaneClosedPayload, PaneOpenedPayload, WorkerExitedPayload, WorkerHeartbeatPayload,
    WorkerLaunchedPayload, WorkerStalePayload, OWNER_LOST,
};
use crate::model::{ObservedProcess, PaneState, WorkerState};

/// One worker run inside one Work, with the pane bound to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRow {
    pub run_id: String,
    pub worker_id: String,
    pub work_id: String,
    /// Thread the launch envelope was scoped to; later markers address it.
    #[serde(default)]
    pub thread_id: String,
    pub worker_state: WorkerState,
    pub provider: Option<String>,
    pub provider_session_ref: Option<String>,
    pub executable_path: Option<String>,
    pub executable_sha256: Option<String>,
    pub cwd: Option<String>,
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub base_commit: Option<String>,
    pub lease_id: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub exit_code: Option<i32>,
    pub failure_code: Option<String>,
    /// Pid the first heartbeat reported.
    #[serde(default)]
    pub pid: Option<u32>,
    /// Platform start identity reported with `pid`, when recorded.
    #[serde(default)]
    pub process_start_id: Option<String>,
    /// Set once restart recovery found this run without a supervisor. It is
    /// history, not an outcome: a later observed exit still ends the run.
    #[serde(default)]
    pub supervision: Option<SupervisionLoss>,
    pub pane_id: Option<String>,
    pub pane_state: Option<PaneState>,
    pub pane_tail: Vec<String>,
    pub pane_dropped_lines: usize,
}

/// What restart recovery recorded when it found a run unsupervised.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisionLoss {
    pub reason: String,
    /// What recovery saw of the process. `alive` means the process kept
    /// running after its supervisor was gone — stale never implies stopped.
    pub process: ObservedProcess,
    pub pid: Option<u32>,
    pub recorded_at: String,
}

impl SupervisionLoss {
    /// Stable display code, e.g. `owner_lost_process_alive`.
    pub fn code(&self) -> String {
        format!("{}_process_{}", self.reason, self.process.as_str())
    }
}

impl RunRow {
    fn new(run_id: String, worker_id: String, work_id: String, thread_id: String) -> Self {
        Self {
            worker_id,
            run_id,
            work_id,
            thread_id,
            worker_state: WorkerState::Starting,
            provider: None,
            provider_session_ref: None,
            executable_path: None,
            executable_sha256: None,
            cwd: None,
            repo_root: None,
            branch: None,
            base_commit: None,
            lease_id: None,
            started_at: None,
            ended_at: None,
            exit_code: None,
            failure_code: None,
            pid: None,
            process_start_id: None,
            supervision: None,
            pane_id: None,
            pane_state: None,
            pane_tail: Vec::new(),
            pane_dropped_lines: 0,
        }
    }
}

/// Fold `events` into the run rows belonging to `work_id`, in first-launch
/// order.
pub fn fold_runs(events: &[OperatorEvent], work_id: &str) -> Vec<RunRow> {
    let mut fold = RunFold::default();
    for event in events {
        if event.work_id.as_deref() == Some(work_id) {
            fold.apply(event, work_id);
        }
    }
    fold.finish()
}

/// Fold every Work's runs in one pass, in first-launch order.
///
/// A run belongs to the Work its launch envelope named; an event naming the
/// same `run_id` under a different Work is not that run's event.
pub fn fold_all_runs(events: &[OperatorEvent]) -> Vec<RunRow> {
    let mut fold = RunFold::default();
    for event in events {
        if let Some(work_id) = event.work_id.as_deref() {
            fold.apply(event, work_id);
        }
    }
    fold.finish()
}

#[derive(Default)]
struct RunFold {
    rows: BTreeMap<String, RunRow>,
    order: Vec<String>,
}

impl RunFold {
    fn row(&mut self, event: &OperatorEvent, work_id: &str) -> Option<&mut RunRow> {
        let run_id = event.run_id.as_deref()?;
        self.rows
            .get_mut(run_id)
            .filter(|row| row.work_id == work_id)
    }

    fn apply(&mut self, event: &OperatorEvent, work_id: &str) {
        match event.event_type {
            OperatorEventType::WorkerLaunched => {
                // The envelope's run_id is one execution; the payload carries
                // the stable worker that owns the prepared lease. A payload
                // that no longer deserializes must not
                // make the run vanish — the envelope already said it exists.
                let Some(run_id) = event.run_id.clone() else {
                    return;
                };
                if !self.rows.contains_key(&run_id) {
                    self.order.push(run_id.clone());
                    self.rows.insert(
                        run_id.clone(),
                        RunRow::new(
                            run_id.clone(),
                            event.actor.id.clone(),
                            work_id.to_string(),
                            event.thread_id.clone(),
                        ),
                    );
                }
                let Some(row) = self.row(event, work_id) else {
                    return;
                };
                row.started_at = Some(event.occurred_at.clone());
                if let Some(payload) = WorkerLaunchedPayload::from_event(event) {
                    row.worker_id = payload.worker_id;
                    row.provider = Some(payload.provider);
                    row.provider_session_ref = payload.provider_session_ref;
                    row.executable_path = Some(payload.executable_path);
                    row.executable_sha256 = Some(payload.executable_sha256);
                    row.cwd = Some(payload.cwd);
                    row.repo_root = Some(payload.repo_root);
                    row.branch = Some(payload.branch);
                    row.base_commit = Some(payload.base_commit);
                    row.lease_id = Some(payload.lease_id);
                }
            }
            OperatorEventType::WorkerHeartbeat => {
                let Some(row) = self.row(event, work_id) else {
                    return;
                };
                if row.worker_state == WorkerState::Starting {
                    row.worker_state = WorkerState::Live;
                }
                if row.pid.is_none() {
                    if let Some(payload) = WorkerHeartbeatPayload::from_event(event) {
                        row.pid = Some(payload.pid);
                        row.process_start_id = payload.process_start_id;
                    }
                }
            }
            OperatorEventType::WorkerExited => {
                let Some(row) = self.row(event, work_id) else {
                    return;
                };
                row.ended_at = Some(event.occurred_at.clone());
                if let Some(payload) = WorkerExitedPayload::from_event(event) {
                    row.exit_code = payload.exit_code;
                    row.failure_code = payload.failure_code.clone();
                    row.worker_state = if payload.failure_code.is_some() {
                        WorkerState::Failed
                    } else {
                        WorkerState::Exited
                    };
                } else {
                    row.worker_state = WorkerState::Exited;
                }
                if row.pane_id.is_some() {
                    row.pane_state = Some(PaneState::for_worker(row.worker_state));
                }
            }
            OperatorEventType::WorkerStale => {
                let Some(row) = self.row(event, work_id) else {
                    return;
                };
                // A recorded ending outranks a later marker, and the first
                // marker's observation stands: recovery marks what it could
                // not vouch for; it never rewrites what the record already says.
                if !matches!(row.worker_state, WorkerState::Starting | WorkerState::Live) {
                    return;
                }
                let payload = WorkerStalePayload::from_event(event);
                row.worker_state = WorkerState::Stale;
                row.supervision = Some(SupervisionLoss {
                    reason: payload
                        .as_ref()
                        .map(|payload| payload.reason.clone())
                        .unwrap_or_else(|| OWNER_LOST.to_string()),
                    process: payload
                        .as_ref()
                        .map(|payload| payload.process)
                        .unwrap_or(ObservedProcess::Unknown),
                    pid: payload.and_then(|payload| payload.pid),
                    recorded_at: event.occurred_at.clone(),
                });
                if row.pane_id.is_some() {
                    row.pane_state = Some(PaneState::for_worker(WorkerState::Stale));
                }
            }
            OperatorEventType::PaneOpened => {
                let Some(payload) = PaneOpenedPayload::from_event(event) else {
                    return;
                };
                // A pane never mints a run: an unlaunched worker stays unknown.
                let Some(row) = self.row(event, work_id) else {
                    return;
                };
                row.pane_id = Some(payload.pane_id);
                row.pane_state = Some(PaneState::for_worker(row.worker_state));
            }
            OperatorEventType::PaneClosed => {
                let Some(payload) = PaneClosedPayload::from_event(event) else {
                    return;
                };
                let Some(row) = self.row(event, work_id) else {
                    return;
                };
                row.pane_tail = payload.tail;
                row.pane_dropped_lines = payload.dropped_lines;
                row.pane_state = Some(PaneState::for_worker(row.worker_state));
            }
            _ => {}
        }
    }

    fn finish(mut self) -> Vec<RunRow> {
        self.order
            .into_iter()
            .filter_map(|id| self.rows.remove(&id))
            .collect()
    }
}
