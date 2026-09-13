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
//! Recovery interprets only rows this service's replay admits. A run touched
//! by evidence this build cannot admit — a newer schema, a row replay rejects,
//! a worker row without a run, a row naming the run under another Work or
//! thread, or an unreadable journal line — may already have an outcome this
//! build cannot read, so it is withheld and reported, never marked.
//!
//! What recovery does is record. It never kills, reattaches, or relaunches a
//! process, and "stale" never claims the process stopped: each marker carries
//! what recovery observed of it — alive, gone, or unknown.

use anyhow::{anyhow, Result};
use heiwa_evidence::{OperatorEvent, OperatorEventType};
use heiwa_session::operator::{
    EventAdmission, OperatorSessionService, RecoveryOutcome, RecoveryPlanner,
};
use heiwa_worker::{
    observe_process, stale_marker, unfinished_runs, ProcessSighting, RunRow, WorkerExitedPayload,
    WorkerHeartbeatPayload, WorkerLaunchedPayload, WorkerStalePayload,
};
use serde::Serialize;
use serde_json::{json, Value};

/// Worker evidence recovery would not interpret, and why. `run_id` and
/// `work_id` say how far the doubt reaches: one run, one Work, or every run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Doubt {
    pub run_id: Option<String>,
    pub work_id: Option<String>,
    pub reason: &'static str,
}

impl Doubt {
    fn of(event: &OperatorEvent, reason: &'static str) -> Self {
        Self {
            run_id: event.run_id.clone(),
            work_id: event.work_id.clone(),
            reason,
        }
    }

    fn reaches(&self, row: &RunRow) -> bool {
        match (&self.run_id, &self.work_id) {
            (Some(run_id), _) => run_id == &row.run_id,
            (None, Some(work_id)) => work_id == &row.work_id,
            (None, None) => true,
        }
    }
}

/// What a recovery pass left alone, beside the markers it appended.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct RunRecoveryReport {
    /// Unfinished runs recovery did not mark, with the doubt that stopped it.
    pub runs_withheld: Vec<Doubt>,
    /// Worker rows this build did not admit, could not parse, or could not
    /// place. Preserved in the journal as written; listed so the uncertainty
    /// is visible.
    pub unadmitted_worker_events: Vec<Doubt>,
    pub unreadable_journal_lines: usize,
}

/// Marks every unfinished run whose evidence it can fully interpret stale,
/// observing each process through `sight`.
pub(crate) struct RunRecovery<S> {
    worker_events: Vec<OperatorEvent>,
    doubts: Vec<Doubt>,
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
            doubts: Vec::new(),
            sight,
        }
    }
}

/// Whether a run-shaping row's payload parses as its typed worker payload.
///
/// Replay admission checks the envelope, not the payload, and the display fold
/// deliberately keeps an envelope-only row visible. Recovery writes evidence,
/// so a payload it cannot parse is uncertainty, not a run it may mark.
fn worker_payload_parses(event: &OperatorEvent) -> bool {
    match event.event_type {
        OperatorEventType::WorkerLaunched => WorkerLaunchedPayload::from_event(event).is_some(),
        OperatorEventType::WorkerHeartbeat => WorkerHeartbeatPayload::from_event(event).is_some(),
        OperatorEventType::WorkerExited => WorkerExitedPayload::from_event(event).is_some(),
        OperatorEventType::WorkerStale => WorkerStalePayload::from_event(event).is_some(),
        _ => true,
    }
}

fn shapes_a_run(event_type: &OperatorEventType) -> bool {
    matches!(
        event_type,
        OperatorEventType::WorkerLaunched
            | OperatorEventType::WorkerHeartbeat
            | OperatorEventType::WorkerExited
            | OperatorEventType::WorkerStale
    )
}

impl<S: FnMut(u32) -> ProcessSighting> RecoveryPlanner for RunRecovery<S> {
    type Report = RunRecoveryReport;

    fn observe(&mut self, event: &OperatorEvent, admission: EventAdmission) {
        // Only what shapes a run row is kept, so a long operator history does
        // not have to fit in memory for recovery to finish.
        if !shapes_a_run(&event.event_type) {
            return;
        }
        match admission {
            EventAdmission::Admitted if !worker_payload_parses(event) => {
                self.doubts
                    .push(Doubt::of(event, "malformed_worker_payload"));
            }
            EventAdmission::Admitted if event.run_id.is_some() => {
                self.worker_events.push(event.clone());
            }
            EventAdmission::Admitted => {
                self.doubts.push(Doubt::of(event, "worker_row_without_run"))
            }
            // The first occurrence already decided; a repeat adds nothing.
            EventAdmission::Duplicate => {}
            EventAdmission::UnsupportedSchema => {
                self.doubts.push(Doubt::of(event, "unsupported_schema"));
            }
            EventAdmission::Rejected => self.doubts.push(Doubt::of(event, "rejected_by_replay")),
        }
    }

    fn plan(mut self, unreadable_lines: usize) -> Result<(Vec<OperatorEvent>, RunRecoveryReport)> {
        let rows = unfinished_runs(&self.worker_events);

        // An admitted row that names an unfinished run under another Work or
        // thread is corrupt evidence about that run; the fold ignores it, so
        // recovery must not act as if it were absent.
        let mut mismatches = Vec::new();
        for event in &self.worker_events {
            let Some(row) = rows
                .iter()
                .find(|row| event.run_id.as_deref() == Some(row.run_id.as_str()))
            else {
                continue;
            };
            if event.work_id.as_deref() != Some(row.work_id.as_str())
                || event.thread_id != row.thread_id
            {
                mismatches.push(Doubt::of(event, "scope_mismatch"));
            }
        }

        let mut report = RunRecoveryReport {
            unadmitted_worker_events: self.doubts.clone(),
            unreadable_journal_lines: unreadable_lines,
            ..RunRecoveryReport::default()
        };
        let occurred_at = chrono::Utc::now().to_rfc3339();
        let mut markers = Vec::new();
        for row in &rows {
            let doubt = if unreadable_lines > 0 {
                Some("unreadable_journal_lines")
            } else {
                self.doubts
                    .iter()
                    .chain(&mismatches)
                    .find(|doubt| doubt.reaches(row))
                    .map(|doubt| doubt.reason)
            };
            if let Some(reason) = doubt {
                report.runs_withheld.push(Doubt {
                    run_id: Some(row.run_id.clone()),
                    work_id: Some(row.work_id.clone()),
                    reason,
                });
                continue;
            }
            let process =
                observe_process(row.pid, row.process_start_id.as_deref(), &mut self.sight);
            markers.push(stale_marker(row, process, &occurred_at, || {
                uuid::Uuid::new_v4().to_string()
            }));
        }
        Ok((markers, report))
    }
}

/// One exclusive recovery pass through `service`, observing real processes.
pub(crate) fn recover(
    service: &OperatorSessionService,
) -> Result<RecoveryOutcome<RunRecoveryReport>> {
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
pub(crate) fn report(outcome: &RecoveryOutcome<RunRecoveryReport>) -> Value {
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
        "runs_withheld": outcome.report.runs_withheld,
        "unadmitted_worker_events": outcome.report.unadmitted_worker_events,
        "unreadable_journal_lines": outcome.report.unreadable_journal_lines,
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
            let boot_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok();
            sighting_from_linux_stat(&stat, boot_id.as_deref())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ProcessSighting::NotRunning,
        Err(_) => ProcessSighting::Uninspectable,
    }
}

/// Read one `/proc/<pid>/stat` line.
///
/// `starttime` counts clock ticks since boot, so it repeats across reboots;
/// the start identity therefore pairs it with the kernel boot identity. When
/// the boot identity cannot be read, the process is running but its identity
/// is unknown — never a match.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn sighting_from_linux_stat(stat: &str, boot_id: Option<&str>) -> ProcessSighting {
    // Fields after the parenthesised command name start at `state`;
    // `starttime` is the twentieth of them.
    let Some((_, fields)) = stat.rsplit_once(')') else {
        return ProcessSighting::Uninspectable;
    };
    let fields: Vec<&str> = fields.split_whitespace().collect();
    if matches!(fields.first(), Some(&"Z") | Some(&"X")) {
        return ProcessSighting::NotRunning;
    }
    let boot_id = boot_id.map(str::trim).filter(|boot_id| !boot_id.is_empty());
    ProcessSighting::Running {
        start_id: match (boot_id, fields.get(19)) {
            (Some(boot_id), Some(ticks)) => Some(format!("{boot_id}:{ticks}")),
            _ => None,
        },
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
    use serde_json::json;

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

    fn only_marked(outcome: &RecoveryOutcome<RunRecoveryReport>) -> Vec<(String, ObservedProcess)> {
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

    fn raw_append(evidence: &Path, event: &OperatorEvent) {
        OperatorJournal::new(evidence.to_path_buf())
            .expect("journal")
            .append(event)
            .expect("raw append");
    }

    fn journal_len(evidence: &Path) -> usize {
        OperatorJournal::new(evidence.to_path_buf())
            .expect("journal")
            .read_after(None, 10_000)
            .expect("read")
            .events
            .len()
    }

    fn launched_row(worker: &WorkerIdentity, run_id: &str) -> OperatorEvent {
        worker_launched_event(worker, run_id, "2026-09-13T00:00:00Z", id)
    }

    fn withheld(outcome: &RecoveryOutcome<RunRecoveryReport>) -> Vec<(String, &'static str)> {
        outcome
            .report
            .runs_withheld
            .iter()
            .map(|doubt| (doubt.run_id.clone().unwrap_or_default(), doubt.reason))
            .collect()
    }

    #[test]
    fn a_future_schema_launch_is_preserved_and_never_marked() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        let mut future = launched_row(&worker, "future-run");
        future.schema_version = 999;
        raw_append(&evidence, &future);
        let before = journal_len(&evidence);

        let outcome = recover(&service(&evidence)).expect("recovery");

        assert!(
            outcome.appended.is_empty(),
            "nothing is invented from a row this build rejects"
        );
        assert_eq!(journal_len(&evidence), before);
        assert_eq!(
            outcome.report.unadmitted_worker_events,
            vec![Doubt {
                run_id: Some("future-run".to_string()),
                work_id: Some(worker.work_id.clone()),
                reason: "unsupported_schema",
            }]
        );
    }

    #[test]
    fn a_future_schema_ending_withholds_its_run() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        let writer = service(&evidence);
        launch(&writer, &worker, "run-1", Some((4242, Some("1.000001"))));
        let mut ending =
            worker_exited_event(&worker, "run-1", Some(0), None, "2026-09-13T00:00:05Z", id);
        ending.schema_version = 999;
        raw_append(&evidence, &ending);

        let outcome = recover(&writer).expect("recovery");

        assert!(
            outcome.appended.is_empty(),
            "a newer ending may exist; never overwrite it"
        );
        assert_eq!(
            withheld(&outcome),
            vec![("run-1".to_string(), "unsupported_schema")]
        );
    }

    #[test]
    fn a_rejected_current_schema_ending_withholds_its_run() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        let writer = service(&evidence);
        launch(&writer, &worker, "run-1", Some((4242, None)));
        // Current schema, but on a thread replay has never seen: rejected.
        let mut ending =
            worker_exited_event(&worker, "run-1", Some(0), None, "2026-09-13T00:00:05Z", id);
        ending.thread_id = "thread-that-does-not-exist".to_string();
        raw_append(&evidence, &ending);

        let outcome = recover(&writer).expect("recovery");

        assert!(outcome.appended.is_empty());
        assert_eq!(
            withheld(&outcome),
            vec![("run-1".to_string(), "rejected_by_replay")]
        );
    }

    #[test]
    fn an_admitted_row_naming_the_run_under_another_work_withholds_it() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        let other = work_with_worker(&evidence);
        let writer = service(&evidence);
        launch(&writer, &worker, "run-1", Some((4242, None)));
        // Replay admits it (its thread exists), but it claims run-1 for another Work.
        writer
            .append_event(worker_exited_event(
                &other,
                "run-1",
                Some(0),
                None,
                "2026-09-13T00:00:05Z",
                id,
            ))
            .expect("admitted mismatch");

        let outcome = recover(&writer).expect("recovery");

        assert!(outcome.appended.is_empty());
        assert_eq!(
            withheld(&outcome),
            vec![("run-1".to_string(), "scope_mismatch")]
        );
    }

    #[test]
    fn an_admitted_worker_row_without_a_run_withholds_its_work() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        let writer = service(&evidence);
        launch(&writer, &worker, "run-1", Some((4242, None)));
        let mut unplaced =
            worker_exited_event(&worker, "run-1", Some(0), None, "2026-09-13T00:00:05Z", id);
        unplaced.run_id = None;
        raw_append(&evidence, &unplaced);

        let outcome = recover(&writer).expect("recovery");

        assert!(outcome.appended.is_empty());
        assert_eq!(
            withheld(&outcome),
            vec![("run-1".to_string(), "worker_row_without_run")]
        );
    }

    /// Recovery through a sighting that records every pid it was asked about.
    fn recover_counting_sightings(
        writer: &OperatorSessionService,
    ) -> (RecoveryOutcome<RunRecoveryReport>, Vec<u32>) {
        let sighted = std::cell::RefCell::new(Vec::new());
        let outcome = writer
            .recover_interrupted_with(RunRecovery::observing(|pid| {
                sighted.borrow_mut().push(pid);
                ProcessSighting::NotRunning
            }))
            .expect("recovery");
        (outcome, sighted.into_inner())
    }

    #[test]
    fn a_current_schema_launch_with_a_malformed_payload_is_preserved_and_never_marked() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        let mut launch_row = launched_row(&worker, "malformed-run");
        launch_row.payload = json!({});
        raw_append(&evidence, &launch_row);
        let before = journal_len(&evidence);

        let (outcome, sighted) = recover_counting_sightings(&service(&evidence));

        assert!(
            outcome.appended.is_empty(),
            "no marker from a payload that does not parse"
        );
        assert!(sighted.is_empty(), "no process observation either");
        assert_eq!(journal_len(&evidence), before);
        assert_eq!(
            outcome.report.unadmitted_worker_events,
            vec![Doubt {
                run_id: Some("malformed-run".to_string()),
                work_id: Some(worker.work_id.clone()),
                reason: "malformed_worker_payload",
            }]
        );
    }

    #[test]
    fn a_malformed_heartbeat_ending_or_marker_withholds_its_run() {
        for (label, malformed) in [
            ("heartbeat", OperatorEventType::WorkerHeartbeat),
            ("exit", OperatorEventType::WorkerExited),
            ("stale", OperatorEventType::WorkerStale),
        ] {
            let runtime = tempfile::tempdir().expect("runtime");
            let evidence = runtime.path().join("evidence");
            let worker = work_with_worker(&evidence);
            let writer = service(&evidence);
            launch(&writer, &worker, "run-1", None);
            let mut row = launched_row(&worker, "run-1");
            row.event_id = id();
            row.event_type = malformed;
            row.payload = json!({ "pid": "not-a-pid", "exit_code": [] });
            raw_append(&evidence, &row);

            let (outcome, sighted) = recover_counting_sightings(&writer);

            assert!(outcome.appended.is_empty(), "{label}: no marker");
            assert!(sighted.is_empty(), "{label}: no process observation");
            assert_eq!(
                withheld(&outcome),
                vec![("run-1".to_string(), "malformed_worker_payload")],
                "{label}"
            );
        }
    }

    #[test]
    fn unreadable_journal_lines_withhold_every_marker() {
        let runtime = tempfile::tempdir().expect("runtime");
        let evidence = runtime.path().join("evidence");
        let worker = work_with_worker(&evidence);
        let writer = service(&evidence);
        launch(&writer, &worker, "run-1", Some((4242, None)));
        let stream = walk(&evidence)
            .into_iter()
            .find(|path| {
                path.file_name()
                    .is_some_and(|name| name == "operator_events.jsonl")
            })
            .expect("operator stream");
        let mut damaged = std::fs::read_to_string(&stream).expect("stream");
        damaged.push_str("{\"record\": not json\n");
        std::fs::write(&stream, damaged).expect("damage stream");

        let outcome = recover(&writer).expect("recovery");

        assert!(
            outcome.appended.is_empty(),
            "an unreadable line may be this run's ending"
        );
        assert_eq!(outcome.report.unreadable_journal_lines, 1);
        assert_eq!(
            withheld(&outcome),
            vec![("run-1".to_string(), "unreadable_journal_lines")]
        );
    }

    fn walk(root: &Path) -> Vec<std::path::PathBuf> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(root).expect("dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(walk(&path));
            } else {
                found.push(path);
            }
        }
        found
    }

    #[test]
    fn linux_start_identity_is_scoped_to_its_boot() {
        let stat =
            "4242 (sleep) S 1 4242 4242 0 -1 4194560 100 0 0 0 0 0 0 0 20 0 1 0 987654 1000 10";
        let on = |boot: Option<&str>| match sighting_from_linux_stat(stat, boot) {
            ProcessSighting::Running { start_id } => start_id,
            other => panic!("expected running, got {other:?}"),
        };
        let first_boot = on(Some("0f7c6c3e-0000-4000-8000-000000000001\n"));
        let second_boot = on(Some("0f7c6c3e-0000-4000-8000-000000000002\n"));
        assert_eq!(
            first_boot.as_deref(),
            Some("0f7c6c3e-0000-4000-8000-000000000001:987654")
        );
        assert_ne!(
            first_boot, second_boot,
            "same pid and ticks on another boot is another process"
        );
        // Recorded on the first boot, sighted on the second: the recorded one is gone.
        assert_eq!(
            observe_process(Some(4242), first_boot.as_deref(), |_| {
                sighting_from_linux_stat(stat, Some("0f7c6c3e-0000-4000-8000-000000000002"))
            }),
            ObservedProcess::Gone
        );
        // No readable boot identity: running, but never a match.
        assert_eq!(on(None), None);
        assert_eq!(
            observe_process(Some(4242), first_boot.as_deref(), |_| {
                sighting_from_linux_stat(stat, None)
            }),
            ObservedProcess::Unknown
        );
        let zombie = stat.replacen(") S ", ") Z ", 1);
        assert_eq!(
            sighting_from_linux_stat(&zombie, Some("boot")),
            ProcessSighting::NotRunning
        );
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
