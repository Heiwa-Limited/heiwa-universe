//! Restart recovery for runs, without I/O.
//!
//! The shell proves nothing supervises the stream (the exclusive activity
//! lease), looks at each run's process, and appends what this module says to.
//! This module decides which runs lost supervision and what a sighting proves;
//! it never spawns, signals, or reads the OS. Recovery records — it does not
//! kill, reattach, or relaunch.

use heiwa_evidence::OperatorEvent;

use crate::events::{worker_stale_event, RunRef};
use crate::model::{ObservedProcess, WorkerState};
use crate::projector::RunRow;

/// What the OS reports about a pid right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessSighting {
    /// No live process holds the pid.
    NotRunning,
    /// A process holds the pid. `start_id` is its platform start identity when
    /// the OS provided one.
    Running { start_id: Option<String> },
    /// The OS refused or failed to describe the pid.
    Uninspectable,
}

/// Decide what a sighting proves about the *recorded* process.
///
/// A pid alone cannot distinguish the recorded process from a later one that
/// reused it, so `Alive` requires matching start identity, and a record
/// without one is `Unknown` whenever something holds the pid.
pub fn observe_process(
    pid: Option<u32>,
    recorded_start_id: Option<&str>,
    sighting: impl FnOnce(u32) -> ProcessSighting,
) -> ObservedProcess {
    let Some(pid) = pid else {
        return ObservedProcess::Unknown;
    };
    match sighting(pid) {
        ProcessSighting::NotRunning => ObservedProcess::Gone,
        ProcessSighting::Uninspectable => ObservedProcess::Unknown,
        ProcessSighting::Running { start_id } => match (recorded_start_id, start_id.as_deref()) {
            (Some(recorded), Some(current)) if recorded == current => ObservedProcess::Alive,
            (Some(_), Some(_)) => ObservedProcess::Gone,
            _ => ObservedProcess::Unknown,
        },
    }
}

/// Runs whose record never reached an ending or a stale marker.
///
/// Only meaningful while the caller holds proof that no supervising process is
/// live; on its own it lists runs that *might* be supervised elsewhere.
pub fn unfinished_runs(events: &[OperatorEvent]) -> Vec<RunRow> {
    crate::projector::fold_all_runs(events)
        .into_iter()
        .filter(|row| matches!(row.worker_state, WorkerState::Starting | WorkerState::Live))
        .collect()
}

/// The stale marker for one unsupervised run, given what recovery observed.
pub fn stale_marker(
    row: &RunRow,
    process: ObservedProcess,
    occurred_at: &str,
    new_event_id: impl FnOnce() -> String,
) -> OperatorEvent {
    worker_stale_event(
        RunRef {
            work_id: &row.work_id,
            thread_id: &row.thread_id,
            run_id: &row.run_id,
            worker_id: &row.worker_id,
        },
        process,
        row.pid,
        occurred_at,
        new_event_id,
    )
}
