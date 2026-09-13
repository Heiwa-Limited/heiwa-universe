//! Restart recovery for worker runs.
//!
//! A run is supervised by the `heiwa` process that spawned it, and that
//! process holds the evidence root's shared activity lease for as long as it
//! lives. Recovery therefore runs only inside
//! [`OperatorSessionService::recover_interrupted_with`]'s exclusive section:
//! winning that lease proves no supervisor is left, so every run the stream
//! still shows starting or live has lost its owner. Healthy owned work is
//! never marked, because its owner's lease makes the section unobtainable.
//!
//! What recovery does is record. It never kills, reattaches, or relaunches a
//! process, and "stale" never claims the process stopped: each marker carries
//! what recovery observed of it — alive, gone, or unknown.

use anyhow::{anyhow, Result};
use heiwa_evidence::{OperatorEvent, OperatorEventType};
use heiwa_session::operator::{OperatorSessionService, RecoveryOutcome, RecoveryPlanner};
use heiwa_worker::{
    observe_process, stale_marker, unfinished_runs, ProcessSighting, WorkerStalePayload,
};
use serde_json::{json, Value};

/// Marks every unfinished run stale, observing each process through `sight`.
pub(crate) struct RunRecovery<S> {
    worker_events: Vec<OperatorEvent>,
    sight: S,
}

impl RunRecovery<fn(u32) -> ProcessSighting> {
    /// Recovery that looks at real processes on this machine.
    pub(crate) fn on_this_machine() -> Self {
        Self::observing(sight_process)
    }
}

impl<S: FnMut(u32) -> ProcessSighting> RunRecovery<S> {
    pub(crate) fn observing(sight: S) -> Self {
        Self {
            worker_events: Vec::new(),
            sight,
        }
    }
}

impl<S: FnMut(u32) -> ProcessSighting> RecoveryPlanner for RunRecovery<S> {
    fn observe(&mut self, event: &OperatorEvent) {
        // Only what builds a run row is kept, so a long operator history does
        // not have to fit in memory for recovery to finish.
        if matches!(
            event.event_type,
            OperatorEventType::WorkerLaunched
                | OperatorEventType::WorkerHeartbeat
                | OperatorEventType::WorkerExited
                | OperatorEventType::WorkerStale
        ) {
            self.worker_events.push(event.clone());
        }
    }

    fn plan(mut self) -> Result<Vec<OperatorEvent>> {
        let occurred_at = chrono::Utc::now().to_rfc3339();
        Ok(unfinished_runs(&self.worker_events)
            .iter()
            .map(|row| {
                let process =
                    observe_process(row.pid, row.process_start_id.as_deref(), &mut self.sight);
                stale_marker(row, process, &occurred_at, || {
                    uuid::Uuid::new_v4().to_string()
                })
            })
            .collect())
    }
}

/// One exclusive recovery pass through `service`, observing real processes.
pub(crate) fn recover(service: &OperatorSessionService) -> Result<RecoveryOutcome> {
    service
        .recover_interrupted_with(RunRecovery::on_this_machine())
        .map_err(|error| {
            if error.to_string().contains("operator_activity_lease_held") {
                anyhow!(
                    "recovery needs every Heiwa writer on this runtime to be stopped: the app runtime \
                     or a running worker still holds operator activity, so its runs are still \
                     supervised. The app runtime recovers automatically when it starts. ({error})"
                )
            } else {
                error
            }
        })
}

/// The machine-readable report of one pass.
pub(crate) fn report(outcome: &RecoveryOutcome) -> Value {
    let runs: Vec<Value> = outcome
        .appended
        .iter()
        .filter(|event| event.event_type == OperatorEventType::WorkerStale)
        .map(|event| {
            let payload = WorkerStalePayload::from_event(event);
            json!({
                "work_id": event.work_id,
                "run_id": event.run_id,
                "worker_id": payload.as_ref().map(|payload| payload.worker_id.clone()),
                "reason": payload.as_ref().map(|payload| payload.reason.clone()),
                "process": payload.as_ref().map(|payload| payload.process.as_str()),
                "pid": payload.as_ref().and_then(|payload| payload.pid),
            })
        })
        .collect();
    json!({
        "interrupted_turns": outcome.interrupted_turns,
        "runs_marked_stale": runs.len(),
        "runs": runs,
    })
}

/// Platform start identity of `pid`, compared only for equality.
pub(crate) fn process_start_id(pid: u32) -> Option<String> {
    match sight_process(pid) {
        ProcessSighting::Running { start_id } => start_id,
        ProcessSighting::NotRunning | ProcessSighting::Uninspectable => None,
    }
}

/// What this machine reports about `pid` right now.
#[cfg(target_os = "macos")]
pub(crate) fn sight_process(pid: u32) -> ProcessSighting {
    // `SZOMB` from <sys/proc.h>: exited, waiting to be reaped.
    const SZOMB: u32 = 5;
    let Ok(pid) = libc::c_int::try_from(pid) else {
        return ProcessSighting::NotRunning;
    };
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    // SAFETY: `info` points to writable, zeroed storage of exactly `size`
    // bytes, and `proc_pidinfo` writes at most the buffer size it is given.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size as libc::c_int,
        )
    };
    if written <= 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ESRCH) => ProcessSighting::NotRunning,
            _ => ProcessSighting::Uninspectable,
        };
    }
    if written as usize != size {
        return ProcessSighting::Uninspectable;
    }
    // SAFETY: the kernel filled all `size` bytes of the zero-initialised struct.
    let info = unsafe { info.assume_init() };
    if info.pbi_status == SZOMB {
        return ProcessSighting::NotRunning;
    }
    ProcessSighting::Running {
        start_id: Some(format!(
            "{}.{:06}",
            info.pbi_start_tvsec, info.pbi_start_tvusec
        )),
    }
}

/// What this machine reports about `pid` right now.
#[cfg(target_os = "linux")]
pub(crate) fn sight_process(pid: u32) -> ProcessSighting {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => {
            // Fields after the parenthesised command name start at `state`;
            // `starttime` (clock ticks after boot) is the twentieth of them.
            let Some((_, fields)) = stat.rsplit_once(')') else {
                return ProcessSighting::Uninspectable;
            };
            let fields: Vec<&str> = fields.split_whitespace().collect();
            if matches!(fields.first(), Some(&"Z") | Some(&"X")) {
                return ProcessSighting::NotRunning;
            }
            ProcessSighting::Running {
                start_id: fields.get(19).map(|field| (*field).to_string()),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ProcessSighting::NotRunning,
        Err(_) => ProcessSighting::Uninspectable,
    }
}

/// What this machine reports about `pid` right now.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn sight_process(_pid: u32) -> ProcessSighting {
    ProcessSighting::Uninspectable
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::{Command, Stdio};

    use heiwa_evidence::OperatorJournal;
    use heiwa_worker::{
        fold_runs, worker_exited_event, worker_heartbeat_event, worker_launched_event,
        ObservedProcess, WorkerIdentity, WorkerState, SCHEMA_VERSION,
    };

    use super::*;

    fn service(evidence: &Path) -> OperatorSessionService {
        OperatorSessionService::new(OperatorJournal::new(evidence.to_path_buf()).expect("journal"))
    }

    /// A durable Work plus the identity a worker in it would record.
    fn work_with_worker(evidence: &Path) -> WorkerIdentity {
        let created =
            crate::cmd::work::create(evidence, "recover runs", "install-1").expect("create Work");
        WorkerIdentity {
            schema_version: SCHEMA_VERSION,
            worker_id: "worker-1".into(),
            work_id: created["work_id"].as_str().expect("work id").into(),
            thread_id: created["primary_thread_id"]
                .as_str()
                .expect("thread")
                .into(),
            provider: "local".into(),
            provider_session_ref: None,
            executable_path: "/bin/sleep".into(),
            executable_sha256: "a".repeat(64),
            cwd: "/tmp".into(),
            repo_root: "/tmp".into(),
            branch: "heiwa/test".into(),
            base_commit: "b".repeat(40),
            lease_id: "lease-1".into(),
            installation_id: "install-1".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    fn id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn launch(
        service: &OperatorSessionService,
        worker: &WorkerIdentity,
        run_id: &str,
        process: Option<(u32, Option<&str>)>,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        service
            .append_event(worker_launched_event(worker, run_id, &now, id))
            .expect("launch");
        if let Some((pid, start_id)) = process {
            service
                .append_event(worker_heartbeat_event(
                    worker, run_id, pid, start_id, &now, id,
                ))
                .expect("heartbeat");
        }
    }

    fn runs(evidence: &Path, work_id: &str) -> Vec<heiwa_worker::RunRow> {
        let journal = OperatorJournal::new(evidence.to_path_buf()).expect("journal");
        let page = journal.read_after(None, 10_000).expect("read");
        let events: Vec<_> = page.events.into_iter().map(|row| row.event).collect();
        fold_runs(&events, work_id)
    }

    fn only_marked(outcome: &RecoveryOutcome) -> Vec<(String, ObservedProcess)> {
        outcome
            .appended
            .iter()
            .filter_map(|event| {
                let payload = WorkerStalePayload::from_event(event)?;
                Some((event.run_id.clone()?, payload.process))
            })
            .collect()
    }

    #[test]
    fn a_run_whose_owner_and_process_are_gone_is_marked_stale_exactly_once() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        let writer = service(&evidence);
        let mut child = Command::new("/usr/bin/true").spawn().expect("spawn");
        let pid = child.id();
        let start_id = process_start_id(pid);
        child.wait().expect("reap");
        launch(&writer, &worker, "run-1", Some((pid, start_id.as_deref())));

        let first = recover(&writer).expect("first recovery");
        // A reaped pid either vanished or now belongs to a different process;
        // both prove the recorded process is not running.
        assert_eq!(
            only_marked(&first),
            vec![("run-1".to_string(), ObservedProcess::Gone)]
        );
        let second = recover(&writer).expect("second recovery");
        assert!(second.appended.is_empty(), "recovery is idempotent");

        let run = &runs(&evidence, &worker.work_id)[0];
        assert_eq!(run.worker_state, WorkerState::Stale);
        assert_eq!(run.exit_code, None);
        assert_eq!(run.ended_at, None);
    }

    #[test]
    fn a_child_that_outlived_its_owner_is_recorded_alive_and_left_running() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        let writer = service(&evidence);
        let mut child = Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();
        let start_id = process_start_id(pid);
        assert!(start_id.is_some(), "this platform reports start identity");
        launch(&writer, &worker, "run-1", Some((pid, start_id.as_deref())));

        let outcome = recover(&writer).expect("recovery");
        let still_running = child.try_wait().expect("poll").is_none();
        let _ = child.kill();
        let _ = child.wait();

        assert_eq!(
            only_marked(&outcome),
            vec![("run-1".to_string(), ObservedProcess::Alive)]
        );
        assert!(
            still_running,
            "recovery records; it never stops the process"
        );
        let loss = runs(&evidence, &worker.work_id)[0]
            .supervision
            .clone()
            .expect("loss recorded");
        assert_eq!(loss.code(), "owner_lost_process_alive");
        assert_eq!(loss.pid, Some(pid));
    }

    #[test]
    fn a_reused_pid_is_gone_and_a_legacy_record_stays_unknown() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        let writer = service(&evidence);
        launch(&writer, &worker, "reused", Some((4242, Some("100.000001"))));
        launch(&writer, &worker, "legacy", Some((4243, None)));
        launch(&writer, &worker, "never-reported", None);

        // Something holds both pids, but it is not the process that was recorded.
        let outcome = writer
            .recover_interrupted_with(RunRecovery::observing(|_| ProcessSighting::Running {
                start_id: Some("200.000002".to_string()),
            }))
            .expect("recovery");

        let marked = only_marked(&outcome);
        assert!(marked.contains(&("reused".to_string(), ObservedProcess::Gone)));
        // Absence of proof is not proof of death.
        assert!(marked.contains(&("legacy".to_string(), ObservedProcess::Unknown)));
        assert!(marked.contains(&("never-reported".to_string(), ObservedProcess::Unknown)));
    }

    #[test]
    fn healthy_owned_work_is_never_marked_stale() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        // The owner stays alive, holding operator activity, exactly as a running
        // `heiwa work run` does while its child executes.
        let owner = service(&evidence);
        launch(&owner, &worker, "run-1", Some((std::process::id(), None)));

        let error = recover(&service(&evidence)).expect_err("owner still holds activity");
        assert!(error.to_string().contains("still supervised"), "{error}");
        assert_eq!(
            runs(&evidence, &worker.work_id)[0].worker_state,
            WorkerState::Live
        );

        drop(owner);
        let outcome = recover(&service(&evidence)).expect("owner gone");
        assert_eq!(outcome.appended.len(), 1, "only once the owner is gone");
    }

    #[test]
    fn finished_runs_are_left_as_recorded() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        let writer = service(&evidence);
        launch(&writer, &worker, "run-1", Some((1, None)));
        let now = chrono::Utc::now().to_rfc3339();
        writer
            .append_event(worker_exited_event(
                &worker,
                "run-1",
                Some(0),
                None,
                &now,
                id,
            ))
            .expect("exit");

        let outcome = recover(&writer).expect("recovery");
        assert!(outcome.appended.is_empty());
        assert_eq!(
            runs(&evidence, &worker.work_id)[0].worker_state,
            WorkerState::Exited
        );
    }
}
