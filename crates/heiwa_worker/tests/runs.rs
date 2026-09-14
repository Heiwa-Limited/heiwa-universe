use heiwa_worker::{
    fold_all_runs, fold_runs, observe_process, pane_closed_event, pane_opened_event,
    worker_exited_event, worker_heartbeat_event, worker_launched_event, worker_stale_event,
    ObservedProcess, PaneIdentity, PaneState, ProcessSighting, RunRef, WorkerIdentity, WorkerState,
    SCHEMA_VERSION,
};

fn identity(work: &str, worker: &str) -> WorkerIdentity {
    WorkerIdentity {
        schema_version: SCHEMA_VERSION,
        worker_id: worker.into(),
        work_id: work.into(),
        thread_id: "thread-1".into(),
        provider: "claude".into(),
        provider_session_ref: None,
        executable_path: "/usr/local/bin/claude".into(),
        executable_sha256: "a".repeat(64),
        cwd: "/tmp/worktrees/w".into(),
        repo_root: "/tmp/repo".into(),
        branch: "heiwa/w".into(),
        base_commit: "b".repeat(40),
        lease_id: "lease-1".into(),
        installation_id: "install-1".into(),
        started_at: "2026-08-26T00:00:00Z".into(),
    }
}

fn pane(work: &str, worker: &str, pane_id: &str) -> PaneIdentity {
    PaneIdentity {
        schema_version: SCHEMA_VERSION,
        pane_id: pane_id.into(),
        work_id: work.into(),
        worker_id: worker.into(),
        cwd: "/tmp/worktrees/w".into(),
        repo_root: "/tmp/repo".into(),
        branch: "heiwa/w".into(),
        opened_at: "2026-08-26T00:00:00Z".into(),
    }
}

fn ids() -> impl FnMut() -> String {
    let mut n = 0;
    move || {
        n += 1;
        format!("e{n}")
    }
}

#[test]
fn a_launched_worker_that_has_not_reported_is_starting() {
    let mut next = ids();
    let events = vec![worker_launched_event(
        &identity("work-1", "worker-1"),
        "run-1",
        "2026-08-26T00:00:00Z",
        &mut next,
    )];
    let runs = fold_runs(&events, "work-1");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].worker_state, WorkerState::Starting);
    assert_eq!(runs[0].exit_code, None);
}

#[test]
fn a_heartbeat_promotes_starting_to_live() {
    let mut next = ids();
    let id = identity("work-1", "worker-1");
    let events = vec![
        worker_launched_event(&id, "run-1", "2026-08-26T00:00:00Z", &mut next),
        worker_heartbeat_event(&id, "run-1", 4242, None, "2026-08-26T00:00:01Z", &mut next),
    ];
    let runs = fold_runs(&events, "work-1");
    assert_eq!(runs[0].worker_state, WorkerState::Live);
}

#[test]
fn a_clean_exit_is_exited_and_a_failure_code_is_failed() {
    let id = identity("work-1", "worker-1");

    let mut next = ids();
    let clean = vec![
        worker_launched_event(&id, "run-1", "2026-08-26T00:00:00Z", &mut next),
        worker_exited_event(
            &id,
            "run-1",
            Some(0),
            None,
            "2026-08-26T00:00:01Z",
            &mut next,
        ),
    ];
    assert_eq!(
        fold_runs(&clean, "work-1")[0].worker_state,
        WorkerState::Exited
    );

    let mut next = ids();
    let broken = vec![
        worker_launched_event(&id, "run-1", "2026-08-26T00:00:00Z", &mut next),
        worker_exited_event(
            &id,
            "run-1",
            None,
            Some("spawn_failed".into()),
            "2026-08-26T00:00:01Z",
            &mut next,
        ),
    ];
    assert_eq!(
        fold_runs(&broken, "work-1")[0].worker_state,
        WorkerState::Failed
    );
}

#[test]
fn a_pane_bound_to_a_worker_that_never_went_live_is_unverified() {
    let mut next = ids();
    let id = identity("work-1", "worker-1");
    let events = vec![
        worker_launched_event(&id, "run-1", "2026-08-26T00:00:00Z", &mut next),
        pane_opened_event(
            &pane("work-1", "worker-1", "pane-1"),
            "run-1",
            "thread-1",
            "2026-08-26T00:00:00Z",
            &mut next,
        ),
    ];
    let runs = fold_runs(&events, "work-1");
    assert_eq!(runs[0].pane_id.as_deref(), Some("pane-1"));
    assert_eq!(runs[0].pane_state, Some(PaneState::Unverified));
}

#[test]
fn a_pane_for_an_unknown_worker_is_not_promoted_to_a_run() {
    let mut next = ids();
    let events = vec![pane_opened_event(
        &pane("work-1", "ghost", "pane-1"),
        "run-ghost",
        "thread-1",
        "2026-08-26T00:00:00Z",
        &mut next,
    )];
    // The spec forbids treating a pane as a verified worker merely because it
    // exists. A pane with no launched worker produces no run row.
    assert!(fold_runs(&events, "work-1").is_empty());
}

#[test]
fn runs_from_another_work_are_excluded() {
    let mut next = ids();
    let events = vec![
        worker_launched_event(
            &identity("work-1", "worker-1"),
            "run-1",
            "2026-08-26T00:00:00Z",
            &mut next,
        ),
        worker_launched_event(
            &identity("work-2", "worker-2"),
            "run-2",
            "2026-08-26T00:00:01Z",
            &mut next,
        ),
    ];
    let runs = fold_runs(&events, "work-1");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].worker_id, "worker-1");
}

#[test]
fn a_closed_pane_reports_its_tail_and_dropped_count() {
    let mut next = ids();
    let id = identity("work-1", "worker-1");
    let p = pane("work-1", "worker-1", "pane-1");
    let events = vec![
        worker_launched_event(&id, "run-1", "2026-08-26T00:00:00Z", &mut next),
        pane_opened_event(&p, "run-1", "thread-1", "2026-08-26T00:00:00Z", &mut next),
        worker_exited_event(
            &id,
            "run-1",
            Some(0),
            None,
            "2026-08-26T00:00:02Z",
            &mut next,
        ),
        pane_closed_event(
            &p,
            "run-1",
            "thread-1",
            vec!["last".into()],
            7,
            "2026-08-26T00:00:03Z",
            &mut next,
        ),
    ];
    let runs = fold_runs(&events, "work-1");
    assert_eq!(runs[0].pane_tail, vec!["last".to_string()]);
    assert_eq!(runs[0].pane_dropped_lines, 7);
    assert_eq!(runs[0].pane_state, Some(PaneState::Done));
}

#[test]
fn an_event_whose_envelope_names_a_work_is_not_second_guessed_from_payload() {
    let mut next = ids();
    let mut event = worker_launched_event(
        &identity("work-1", "worker-1"),
        "run-1",
        "2026-08-26T00:00:00Z",
        &mut next,
    );
    event.payload = serde_json::json!({ "worker_id": "worker-1" });
    // A payload that no longer deserializes fully must not silently vanish; the
    // envelope still says this Work has a worker.
    let runs = fold_runs(&[event], "work-1");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].worker_id, "worker-1");
    assert_eq!(runs[0].provider, None);
}

#[test]
fn a_stale_marker_changes_only_its_run_and_never_rewrites_an_ending() {
    let mut next = ids();
    let id = identity("work-1", "worker-1");
    let events = vec![
        // The prepared worker's first invocation completes.
        worker_launched_event(&id, "run-1", "2026-09-13T00:00:00Z", &mut next),
        worker_heartbeat_event(
            &id,
            "run-1",
            4242,
            Some("1757721600.000001"),
            "2026-09-13T00:00:01Z",
            &mut next,
        ),
        worker_exited_event(
            &id,
            "run-1",
            Some(0),
            None,
            "2026-09-13T00:00:02Z",
            &mut next,
        ),
        // Its second invocation loses the process that supervised it.
        worker_launched_event(&id, "run-2", "2026-09-13T00:01:00Z", &mut next),
        worker_heartbeat_event(
            &id,
            "run-2",
            4243,
            Some("1757721660.000002"),
            "2026-09-13T00:01:01Z",
            &mut next,
        ),
        worker_stale_event(
            RunRef::of(&id, "run-2"),
            ObservedProcess::Alive,
            Some(4243),
            "2026-09-13T01:00:00Z",
            &mut next,
        ),
        // A marker addressed to the finished run must not rewrite its outcome.
        worker_stale_event(
            RunRef::of(&id, "run-1"),
            ObservedProcess::Gone,
            Some(4242),
            "2026-09-13T01:00:00Z",
            &mut next,
        ),
    ];

    let runs = fold_runs(&events, "work-1");
    assert_eq!(
        runs.len(),
        2,
        "both invocations of one worker survive the fold"
    );

    let first = &runs[0];
    assert_eq!(first.run_id, "run-1");
    assert_eq!(first.worker_state, WorkerState::Exited);
    assert_eq!(first.exit_code, Some(0));
    assert!(
        first.supervision.is_none(),
        "a known ending is never reinterpreted"
    );

    let second = &runs[1];
    assert_eq!(second.run_id, "run-2");
    assert_eq!(
        second.worker_id, first.worker_id,
        "same worker, separate runs"
    );
    assert_eq!(second.worker_state, WorkerState::Stale);
    // Stale is loss of supervision, not an ending: nothing observed an exit.
    assert_eq!(second.exit_code, None);
    assert_eq!(second.ended_at, None);
    assert_eq!(second.failure_code, None);
    let loss = second
        .supervision
        .as_ref()
        .expect("the loss is recorded on the run");
    assert_eq!(loss.process, ObservedProcess::Alive);
    assert_eq!(loss.pid, Some(4243));
    assert_eq!(loss.code(), "owner_lost_process_alive");
}

#[test]
fn a_stale_run_keeps_its_first_observation_and_its_pane_reads_stale() {
    let mut next = ids();
    let id = identity("work-1", "worker-1");
    let p = pane("work-1", "worker-1", "pane-1");
    let events = vec![
        worker_launched_event(&id, "run-1", "2026-09-13T00:00:00Z", &mut next),
        pane_opened_event(&p, "run-1", "thread-1", "2026-09-13T00:00:00Z", &mut next),
        worker_heartbeat_event(&id, "run-1", 4242, None, "2026-09-13T00:00:01Z", &mut next),
        worker_stale_event(
            RunRef::of(&id, "run-1"),
            ObservedProcess::Unknown,
            Some(4242),
            "2026-09-13T01:00:00Z",
            &mut next,
        ),
        worker_stale_event(
            RunRef::of(&id, "run-1"),
            ObservedProcess::Gone,
            Some(4242),
            "2026-09-13T02:00:00Z",
            &mut next,
        ),
    ];
    let run = &fold_runs(&events, "work-1")[0];
    assert_eq!(run.worker_state, WorkerState::Stale);
    assert_eq!(run.pane_state, Some(PaneState::Stale));
    let loss = run.supervision.as_ref().expect("loss");
    assert_eq!(
        loss.process,
        ObservedProcess::Unknown,
        "the first recorded observation stands"
    );
    assert_eq!(loss.recorded_at, "2026-09-13T01:00:00Z");
    assert_eq!(run.pid, Some(4242));
}

#[test]
fn an_exit_recorded_after_a_stale_marker_is_the_outcome() {
    let mut next = ids();
    let id = identity("work-1", "worker-1");
    let events = vec![
        worker_launched_event(&id, "run-1", "2026-09-13T00:00:00Z", &mut next),
        worker_heartbeat_event(&id, "run-1", 4242, None, "2026-09-13T00:00:01Z", &mut next),
        worker_stale_event(
            RunRef::of(&id, "run-1"),
            ObservedProcess::Alive,
            Some(4242),
            "2026-09-13T01:00:00Z",
            &mut next,
        ),
        worker_exited_event(
            &id,
            "run-1",
            Some(0),
            None,
            "2026-09-13T01:00:05Z",
            &mut next,
        ),
    ];
    let run = &fold_runs(&events, "work-1")[0];
    assert_eq!(
        run.worker_state,
        WorkerState::Exited,
        "an observed ending outranks lost supervision"
    );
    assert!(run.supervision.is_some(), "the loss stays in history");
}

#[test]
fn every_work_folds_in_one_pass_without_crossing_runs() {
    let mut next = ids();
    let a = identity("work-a", "worker-a");
    let b = identity("work-b", "worker-b");
    let events = vec![
        worker_launched_event(&a, "run-a", "2026-09-13T00:00:00Z", &mut next),
        worker_launched_event(&b, "run-b", "2026-09-13T00:00:00Z", &mut next),
        worker_exited_event(
            &b,
            "run-b",
            Some(0),
            None,
            "2026-09-13T00:00:02Z",
            &mut next,
        ),
        // An exit that claims run-a under another Work is not run-a's exit.
        worker_exited_event(
            &b,
            "run-a",
            Some(1),
            None,
            "2026-09-13T00:00:03Z",
            &mut next,
        ),
    ];
    let runs = fold_all_runs(&events);
    assert_eq!(runs.len(), 2);
    let run_a = runs
        .iter()
        .find(|row| row.run_id == "run-a")
        .expect("run-a");
    assert_eq!(run_a.work_id, "work-a");
    assert_eq!(run_a.thread_id, "thread-1");
    assert_eq!(run_a.worker_state, WorkerState::Starting);
    let run_b = runs
        .iter()
        .find(|row| row.run_id == "run-b")
        .expect("run-b");
    assert_eq!(run_b.worker_state, WorkerState::Exited);
}

#[test]
fn process_observation_needs_proof_before_it_says_alive_or_gone() {
    let running = |start: Option<&str>| {
        let start = start.map(str::to_string);
        move |_pid: u32| ProcessSighting::Running {
            start_id: start.clone(),
        }
    };
    // No pid was ever recorded: nothing to look for.
    assert_eq!(
        observe_process(None, None, |_| ProcessSighting::NotRunning),
        ObservedProcess::Unknown
    );
    // No process holds the pid: the recorded process cannot be running.
    assert_eq!(
        observe_process(Some(7), Some("1.0"), |_| ProcessSighting::NotRunning),
        ObservedProcess::Gone
    );
    assert_eq!(
        observe_process(Some(7), None, |_| ProcessSighting::NotRunning),
        ObservedProcess::Gone
    );
    // Same pid, same start identity: the orphan is still running.
    assert_eq!(
        observe_process(Some(7), Some("1.0"), running(Some("1.0"))),
        ObservedProcess::Alive
    );
    // Same pid, different start identity: the pid was reused.
    assert_eq!(
        observe_process(Some(7), Some("1.0"), running(Some("2.0"))),
        ObservedProcess::Gone
    );
    // A legacy record without start identity cannot rule out reuse.
    assert_eq!(
        observe_process(Some(7), None, running(Some("2.0"))),
        ObservedProcess::Unknown
    );
    // A process the OS will not describe is not evidence either way.
    assert_eq!(
        observe_process(Some(7), Some("1.0"), |_| ProcessSighting::Uninspectable),
        ObservedProcess::Unknown
    );
}
