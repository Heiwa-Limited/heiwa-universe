//! `heiwa work` — durable Work on this installation.
//!
//! Read and create only. Work is appended through `OperatorSessionService`, so
//! this command adds no second writer; it resolves the runtime root once and
//! hands it down.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use heiwa_evidence::{CursorError, CursorEvent, OperatorEvent, OperatorJournal};
use heiwa_session::operator::OperatorSessionService;
use heiwa_work::{
    build_work_session, fold, work_created_event, Work, WorkId, WorkProjection,
    WorkSessionBuildOptions, WorkSessionSnapshotV1,
};

use crate::cmd::args::{has_flag, optional_value, positionals};
use crate::output::{self, CliError, STREAM_SCHEMA};

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("list") | Some("status") | None => list(args),
        Some("create") => create_command(&args[1..]),
        Some("show") => show_command(&args[1..]),
        Some("watch") => watch_command(&args[1..]),
        Some("run") => crate::cmd::worker::run(&args[1..]),
        Some("recover") => recover_command(&args[1..]),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(CliError::usage(format!("unknown work command: {other}"))
            .with_hint("run `heiwa work --help`")
            .into()),
    }
}

fn print_help() {
    println!("heiwa work — durable Work on this installation");
    println!();
    println!("  heiwa work list [--json]              what Work exists and where it stands");
    println!("  heiwa work create <intent> [--json]   open a new Work and its primary thread");
    println!("  heiwa work show <work-id> [--json]    bounded session truth for one Work");
    println!("  heiwa work show <work-id> --surface home|work|agent|all");
    println!("                                        surface views of one snapshot, as JSON");
    println!("  heiwa work watch <work-id> [--since <cursor>] [--once] [--json]");
    println!("                                        stream this Work's events; resumable");
    println!("  heiwa work run <work-id> -- <cmd>     run a provider-owned worker in its worktree");
    println!(
        "  heiwa work recover [--json]           record runs whose supervising process is gone"
    );
}

fn service(root: &Path) -> Result<OperatorSessionService> {
    Ok(OperatorSessionService::new(
        OperatorJournal::new(root.to_path_buf()).map_err(|error| anyhow!("{error}"))?,
    ))
}

fn list(args: &[String]) -> Result<()> {
    let paths = heiwa_config::HeiwaPaths::resolve();
    let summary = summarize(&paths.evidence_dir)?;
    let next = if summary["work"].as_array().is_none_or(Vec::is_empty) {
        vec!["heiwa work create \"<what you want done>\"".to_string()]
    } else {
        Vec::new()
    };
    output::emit(has_flag(args, "--json"), summary, &next, render_list)
}

fn render_list(summary: &Value) {
    let works = summary["work"].as_array().cloned().unwrap_or_default();
    if works.is_empty() {
        println!("no Work on this installation yet");
        println!("  run `heiwa work create \"<what you want done>\"`");
    } else {
        for work in &works {
            println!(
                "{}  {}  rev {}",
                work["work_id"].as_str().unwrap_or("?"),
                work["status"].as_str().unwrap_or("?"),
                work["revision"].as_u64().unwrap_or(0),
            );
            println!("  {}", work["intent"].as_str().unwrap_or(""));
        }
    }
    let skipped = summary["skipped_events"].as_u64().unwrap_or(0);
    if skipped > 0 {
        println!();
        println!("! {skipped} work event(s) could not be folded; run `heiwa doctor` for detail");
    }
}

fn create_command(args: &[String]) -> Result<()> {
    let intent = positionals(args, &[])
        .first()
        .copied()
        .ok_or_else(|| CliError::usage("usage: heiwa work create \"<intent>\" [--json]"))?;
    let paths = heiwa_config::HeiwaPaths::resolve();
    let identity = heiwa_identity::load_from(&paths.runtime_root)
        .map_err(|error| anyhow!("{error}"))?
        .ok_or_else(|| {
            CliError::failure("no local identity on this installation")
                .with_hint("run `heiwa setup` before creating Work")
        })?;

    let created = create(&paths.evidence_dir, intent, &identity.installation_id)?;
    let work_id = created["work_id"].as_str().unwrap_or("?").to_string();
    let next = vec![
        format!("heiwa work show {work_id}"),
        format!("heiwa work watch {work_id}"),
    ];
    output::emit(has_flag(args, "--json"), created, &next, |created| {
        println!("opened {}", created["work_id"].as_str().unwrap_or("?"));
        println!("  {}", created["intent"].as_str().unwrap_or(""));
    })
}

fn show_command(args: &[String]) -> Result<()> {
    let json = has_flag(args, "--json");
    let surface = optional_value(args, "--surface")?;
    let work_id = positionals(args, &["--surface"])
        .first()
        .copied()
        .ok_or_else(|| {
            CliError::usage("usage: heiwa work show <work-id> [--json | --surface <name>]")
        })?;
    let paths = heiwa_config::HeiwaPaths::resolve();
    if find(&paths.evidence_dir, work_id)?.is_none() {
        return Err(CliError::not_found(format!("no Work {work_id} on this installation"))
            .with_hint("list Work with `heiwa work list`")
            .into());
    }
    let epoch_seed = format!("cli-{}", uuid::Uuid::new_v4());
    if let Some(surface) = surface {
        let rendered = if surface == "all" {
            surfaces_json(&paths.evidence_dir, work_id, &epoch_seed)?
        } else {
            let snapshot = session(&paths.evidence_dir, work_id, &epoch_seed)?;
            let view = heiwa_work::view_for(&snapshot, surface).ok_or_else(|| {
                CliError::usage(format!(
                    "unknown surface {surface}; expected home, work, agent, or all"
                ))
            })?;
            serde_json::to_value(view)?
        };
        return output::emit(json, rendered, &[], |rendered| {
            println!("{}", serde_json::to_string_pretty(rendered).unwrap_or_default());
        });
    }
    let snapshot = session(&paths.evidence_dir, work_id, &epoch_seed)?;
    if json {
        return output::emit(true, serde_json::to_value(&snapshot)?, &[], |_| {});
    }

    println!(
        "{}  rev {}  projection {}",
        snapshot.work_id, snapshot.work_revision, snapshot.projection_revision
    );
    if let Some(work) = snapshot
        .collections
        .get("work")
        .and_then(|rows| rows.get(&snapshot.work_id))
    {
        println!("  {}", work["intent"].as_str().unwrap_or(""));
        println!("  status: {}", work["status"].as_str().unwrap_or("unknown"));
    }
    // Runs get their own block rather than a count: what ran, as what
    // identity, and how it ended is the question `heiwa work show` exists to
    // answer once a worker has touched the Work.
    if let Some(runs) = snapshot.collections.get("runs") {
        for (run_id, run) in runs {
            println!(
                "  run {run_id}  {}  {}",
                run["worker_state"].as_str().unwrap_or("unknown"),
                run["provider"].as_str().unwrap_or("-")
            );
            println!("    cwd  {}", run["cwd"].as_str().unwrap_or("?"));
            if run["worker_state"].as_str() == Some("stale") {
                println!("    {}", describe_supervision_loss(run));
            } else {
                match run["exit_code"].as_i64() {
                    Some(code) => println!("    exit {code}"),
                    None if run["ended_at"].is_null() => println!("    exit (still running)"),
                    None => println!("    exit (signalled)"),
                }
            }
            if let Some(pane) = run["pane_id"].as_str() {
                println!(
                    "    pane {pane}  {}",
                    run["pane_state"].as_str().unwrap_or("unknown")
                );
            }
        }
    }
    for name in [
        "threads",
        "workspace",
        "runs",
        "approvals",
        "actions",
        "artifacts",
        "tests",
        "receipts",
        "blockers",
    ] {
        let count = snapshot.collections.get(name).map_or(0, BTreeMap::len);
        let omitted = snapshot
            .truncated_collections
            .get(name)
            .copied()
            .unwrap_or(0);
        if count > 0 || omitted > 0 {
            println!("  {name}: {count} visible, {omitted} omitted");
        }
    }
    Ok(())
}

/// One line for a stale run that never lets "stale" read as "stopped".
fn describe_supervision_loss(run: &Value) -> String {
    let loss = &run["supervision"];
    let pid = loss["pid"]
        .as_u64()
        .map(|pid| format!(" (pid {pid})"))
        .unwrap_or_default();
    match loss["process"].as_str() {
        Some("alive") => format!(
            "supervision lost: process still running unsupervised{pid}; recovery did not stop it"
        ),
        Some("gone") => {
            format!("supervision lost: process no longer running{pid}; no exit was observed")
        }
        _ => format!("supervision lost: process state unknown{pid}"),
    }
}

fn recover_command(args: &[String]) -> Result<()> {
    let paths = heiwa_config::HeiwaPaths::resolve();
    let outcome = crate::cmd::recover::recover(&service(&paths.evidence_dir)?)?;
    let report = crate::cmd::recover::report(&outcome);
    output::emit(has_flag(args, "--json"), report, &[], render_recovery)
}

fn render_recovery(report: &Value) {
    println!(
        "recovered {} interrupted turn(s); {} run(s) marked stale",
        report["interrupted_turns"].as_u64().unwrap_or(0),
        report["runs_marked_stale"].as_u64().unwrap_or(0)
    );
    for run in report["runs"].as_array().into_iter().flatten() {
        let described = describe_supervision_loss(&json!({ "supervision": run }));
        println!(
            "  run {}  {}  {described}",
            run["run_id"].as_str().unwrap_or("?"),
            run["work_id"].as_str().unwrap_or("?")
        );
    }
    for run in report["runs_withheld"].as_array().into_iter().flatten() {
        println!(
            "  run {}  {}  not marked: {}",
            run["run_id"].as_str().unwrap_or("?"),
            run["work_id"].as_str().unwrap_or("?"),
            run["reason"].as_str().unwrap_or("unknown evidence")
        );
    }
    let unadmitted = report["unadmitted_worker_events"]
        .as_array()
        .map_or(0, Vec::len);
    let unreadable = report["unreadable_journal_lines"].as_u64().unwrap_or(0);
    if unadmitted > 0 || unreadable > 0 {
        println!(
            "! {unadmitted} worker row(s) this build does not admit and {unreadable} unreadable journal line(s) were preserved uninterpreted"
        );
    }
}

/// Which events belong to one Work while it is being watched.
pub(crate) struct WatchScope {
    work_id: String,
    threads: BTreeSet<String>,
}

impl WatchScope {
    pub(crate) fn new(work: &Work) -> Self {
        let mut threads: BTreeSet<String> = work.related_thread_ids.iter().cloned().collect();
        threads.insert(work.primary_thread_id.clone());
        Self {
            work_id: work.work_id.as_str().to_string(),
            threads,
        }
    }

    /// `"work"` when the event carries this Work's id. Its thread then joins
    /// the scope, so threads linked after the watch began stay visible.
    /// `"thread"` marks an *unscoped* event in one of those threads; it is
    /// shown rather than hidden. Two Works may share a thread, so an event
    /// explicitly scoped to another Work is never this Work's context.
    pub(crate) fn admit(&mut self, event: &OperatorEvent) -> Option<&'static str> {
        match event.work_id.as_deref() {
            Some(work_id) if work_id == self.work_id => {
                self.threads.insert(event.thread_id.clone());
                Some("work")
            }
            Some(_) => None,
            None if self.threads.contains(&event.thread_id) => Some("thread"),
            None => None,
        }
    }
}

pub(crate) enum WatchStep {
    /// This Work's stream lines, and the journal's own resume cursor. That
    /// cursor never regresses: an empty page echoes the input cursor.
    Page {
        lines: Vec<Value>,
        cursor: Option<String>,
    },
    /// The cursor no longer fits the stream: it was repaired, replaced, or
    /// compacted, or the cursor came from another stream or an older binary.
    Resync { reason: String },
}

/// Decode-stage rejections from `heiwa_evidence`: the value was never a
/// cursor. Every other `InvalidCursor` means a real cursor has expired.
fn cursor_is_malformed(reason: &str) -> bool {
    reason.starts_with("cursor is not valid base64")
        || reason.starts_with("cursor payload is malformed")
}

/// Read one journal page after `since` and keep this Work's events.
pub(crate) fn watch_page(
    root: &Path,
    scope: &mut WatchScope,
    since: Option<&str>,
    limit: usize,
) -> Result<WatchStep> {
    let journal = OperatorJournal::new(root.to_path_buf()).map_err(|error| anyhow!("{error}"))?;
    let page = match journal.read_after(since, limit) {
        Ok(page) => page,
        Err(CursorError::InvalidCursor { reason }) if cursor_is_malformed(&reason) => {
            return Err(CliError::usage(format!("invalid --since cursor: {reason}"))
                .with_hint("pass a cursor printed by `heiwa work watch`")
                .into());
        }
        Err(CursorError::InvalidCursor { reason }) => return Ok(WatchStep::Resync { reason }),
        Err(other) => return Err(anyhow!("{other}")),
    };
    let lines = page
        .events
        .into_iter()
        .filter_map(|CursorEvent { cursor, event }| {
            scope.admit(&event).map(|scope_name| {
                json!({
                    "schema": STREAM_SCHEMA,
                    "type": "event",
                    "scope": scope_name,
                    "cursor": cursor,
                    "event": event,
                })
            })
        })
        .collect();
    Ok(WatchStep::Page {
        lines,
        cursor: page.next_cursor,
    })
}

fn watch_command(args: &[String]) -> Result<()> {
    const PAGE_SIZE: usize = 256;
    const MAX_RESYNCS: usize = 3;
    const POLL: std::time::Duration = std::time::Duration::from_millis(500);

    let work_id = positionals(args, &["--since"])
        .first()
        .copied()
        .ok_or_else(|| {
            CliError::usage("usage: heiwa work watch <work-id> [--since <cursor>] [--once] [--json]")
        })?;
    let json = has_flag(args, "--json");
    let once = has_flag(args, "--once");
    let paths = heiwa_config::HeiwaPaths::resolve();
    let work = find(&paths.evidence_dir, work_id)?.ok_or_else(|| {
        CliError::not_found(format!("no Work {work_id} on this installation"))
            .with_hint("list Work with `heiwa work list`")
    })?;

    let mut scope = WatchScope::new(&work);
    let mut cursor = optional_value(args, "--since")?.map(str::to_string);
    let mut resyncs = 0usize;
    loop {
        match watch_page(&paths.evidence_dir, &mut scope, cursor.as_deref(), PAGE_SIZE)? {
            WatchStep::Resync { reason } => {
                resyncs += 1;
                if resyncs > MAX_RESYNCS {
                    return Err(CliError::failure(format!(
                        "the operator stream kept changing under this watch: {reason}"
                    ))
                    .with_hint("retry once the runtime is idle")
                    .into());
                }
                if !print_watch_resync(&reason, json)? {
                    return Ok(());
                }
                cursor = None;
                scope = WatchScope::new(&work);
            }
            WatchStep::Page { lines, cursor: next } => {
                resyncs = 0;
                let advanced = next != cursor;
                for line in &lines {
                    if !print_watch_line(line, json)? {
                        return Ok(());
                    }
                }
                cursor = next;
                if advanced {
                    continue;
                }
                if once {
                    return print_watch_end(work_id, cursor.as_deref(), json);
                }
                std::thread::sleep(POLL);
            }
        }
    }
}

/// `Ok(false)`: the reader closed the pipe, so stop quietly.
fn print_watch_line(line: &Value, json: bool) -> Result<bool> {
    let text = if json {
        line.to_string()
    } else {
        let event = &line["event"];
        format!(
            "{}  {:<22} {:<6} {}",
            event["occurred_at"].as_str().unwrap_or("?"),
            event["event_type"].as_str().unwrap_or("?"),
            line["scope"].as_str().unwrap_or("?"),
            event["actor"]["id"].as_str().unwrap_or("?"),
        )
    };
    Ok(output::print_line(&text)?)
}

fn print_watch_resync(reason: &str, json: bool) -> Result<bool> {
    let text = if json {
        json!({
            "schema": STREAM_SCHEMA,
            "type": "resync",
            "reason": reason,
            "cursor": Value::Null,
        })
        .to_string()
    } else {
        format!("! the operator stream was rewritten ({reason}); replaying this Work from the start")
    };
    Ok(output::print_line(&text)?)
}

fn print_watch_end(work_id: &str, cursor: Option<&str>, json: bool) -> Result<()> {
    let text = if json {
        json!({
            "schema": STREAM_SCHEMA,
            "type": "end",
            "reason": "caught_up",
            "cursor": cursor,
        })
        .to_string()
    } else if let Some(cursor) = cursor {
        format!("caught up; resume with: heiwa work watch {work_id} --since {cursor}")
    } else {
        "caught up; no events yet".to_string()
    };
    output::print_line(&text)?;
    Ok(())
}

/// Create one Work and its primary thread, atomically from the caller's view:
/// the thread exists before the event that names it.
pub(crate) fn create(root: &Path, intent: &str, installation_id: &str) -> Result<Value> {
    let service = service(root)?;
    let work_id = WorkId::generate(|| uuid::Uuid::new_v4().to_string());
    let thread_id = format!("thread-{}", uuid::Uuid::new_v4());
    service
        .ensure_thread(&thread_id)
        .map_err(|error| anyhow!("{error}"))?;

    let occurred_at = chrono::Utc::now().to_rfc3339();
    service
        .append_event(work_created_event(
            &work_id,
            &thread_id,
            intent,
            installation_id,
            &occurred_at,
            || uuid::Uuid::new_v4().to_string(),
        ))
        .map_err(|error| anyhow!("{error}"))?;

    Ok(json!({
        "work_id": work_id.as_str(),
        "primary_thread_id": thread_id,
        "intent": intent,
        "created_at": occurred_at,
    }))
}

/// Every Work visible on this installation, plus damage found while folding.
pub(crate) fn summarize(root: &Path) -> Result<Value> {
    let projection = project(root)?;
    let work: Vec<Value> = projection.all().map(work_row).collect();

    Ok(json!({
        "work": work,
        "skipped_events": projection.skipped_events,
    }))
}

/// A bounded Work catalog, most recently updated first.
///
/// Every Work is folded, so `total` and `skipped_events` describe the whole
/// installation even when only `limit` rows are returned; `truncated` counts
/// the rows left out so a reader never mistakes a bound for completeness.
pub(crate) fn catalog_json(root: &Path, limit: usize) -> Result<Value> {
    let projection = project(root)?;
    let mut works: Vec<&Work> = projection.all().collect();
    works.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.work_id.as_str().cmp(right.work_id.as_str()))
    });
    let total = works.len();
    let rows: Vec<Value> = works.into_iter().take(limit).map(work_row).collect();
    Ok(json!({
        "work": rows,
        "total": total,
        "truncated": total.saturating_sub(limit),
        "skipped_events": projection.skipped_events,
    }))
}

fn work_row(work: &Work) -> Value {
    json!({
        "work_id": work.work_id.as_str(),
        "intent": work.intent,
        "status": work.status,
        "revision": work.revision,
        "primary_thread_id": work.primary_thread_id,
        "related_thread_ids": work.related_thread_ids,
        "origin_installation_id": work.origin_installation_id,
        "replicable": work.is_replicable(),
        "created_at": work.created_at,
        "updated_at": work.updated_at,
    })
}

/// Fold the operator stream in durable append order.
///
/// Reading each thread separately destroys cross-thread ordering: a later
/// `work_linked` can be visited before its earlier `work_created`, making valid
/// history look damaged. The journal cursor is the order authority.
pub(crate) fn project(root: &Path) -> Result<WorkProjection> {
    let rows = read_rows(root)?;
    Ok(fold(
        &rows.into_iter().map(|row| row.event).collect::<Vec<_>>(),
    ))
}

pub(crate) fn session(
    root: &Path,
    work_id: &str,
    epoch_seed: &str,
) -> Result<WorkSessionSnapshotV1> {
    let rows = read_rows(root)?;
    build_work_session(
        &rows,
        work_id,
        WorkSessionBuildOptions::new(epoch_seed, 256),
    )
    .map_err(|error| anyhow!(error))
}

fn read_rows(root: &Path) -> Result<Vec<CursorEvent>> {
    const PAGE_SIZE: usize = 256;

    let journal = OperatorJournal::new(root.to_path_buf()).map_err(|error| anyhow!("{error}"))?;
    let mut cursor: Option<String> = None;
    let mut rows = Vec::new();
    loop {
        let page = journal
            .read_after(cursor.as_deref(), PAGE_SIZE)
            .map_err(|error| anyhow!("{error}"))?;
        if page.events.is_empty() {
            break;
        }
        cursor = page.next_cursor;
        rows.extend(page.events);
    }
    Ok(rows)
}

pub(crate) fn find(root: &Path, work_id: &str) -> Result<Option<Work>> {
    Ok(project(root)?.work(work_id).cloned())
}

/// Home, Work, and Agent views of one Work, all from one snapshot so they
/// cannot disagree. The CLI and the app API both serve exactly this.
pub(crate) fn surfaces_json(root: &Path, work_id: &str, epoch_seed: &str) -> Result<Value> {
    let snapshot = session(root, work_id, epoch_seed)?;
    Ok(json!({ "surfaces": heiwa_work::surfaces(&snapshot) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use heiwa_evidence::{
        OperatorActor, OperatorEvent, OperatorEventType, OperatorRisk, OperatorSensitivity,
        OPERATOR_EVENT_SCHEMA_VERSION,
    };

    fn root() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn scoped_event(
        work_id: &str,
        thread_id: &str,
        turn_id: &str,
        call_id: Option<&str>,
        event_type: OperatorEventType,
        payload: Value,
    ) -> OperatorEvent {
        OperatorEvent {
            schema_version: OPERATOR_EVENT_SCHEMA_VERSION,
            event_id: format!("evt-{}", uuid::Uuid::new_v4()),
            thread_id: thread_id.to_string(),
            turn_id: Some(turn_id.to_string()),
            run_id: None,
            call_id: call_id.map(str::to_string),
            work_id: Some(work_id.to_string()),
            event_type,
            occurred_at: chrono::Utc::now().to_rfc3339(),
            actor: OperatorActor {
                kind: "runtime".to_string(),
                id: "work-command-test".to_string(),
            },
            risk_class: OperatorRisk::Low,
            sensitivity: OperatorSensitivity::LocalPrivate,
            parent_event_id: None,
            correlation_id: call_id.map(str::to_string),
            source_refs: vec![],
            evidence_refs: vec![],
            payload,
        }
    }

    #[test]
    fn a_fresh_root_lists_no_work() {
        let dir = root();
        let summary = summarize(dir.path()).expect("summarize");
        assert_eq!(summary["work"].as_array().map(Vec::len), Some(0));
        assert!(summary.get("errors").is_none(), "{summary}");
    }

    #[test]
    fn creating_work_makes_it_listable_and_replayable() {
        let dir = root();
        let created =
            create(dir.path(), "prepare the release", "installation-1").expect("create work");
        let work_id = created["work_id"].as_str().expect("work_id").to_string();
        assert!(work_id.starts_with("work-"), "{work_id}");

        let summary = summarize(dir.path()).expect("summarize");
        let listed = summary["work"].as_array().expect("array");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["work_id"], work_id);
        assert_eq!(listed[0]["intent"], "prepare the release");
        assert_eq!(listed[0]["revision"], 1);
        assert_eq!(
            listed[0]["replicable"], false,
            "work created before enrolment must not claim mesh reach"
        );
    }

    #[test]
    fn showing_work_reuses_the_canonical_session_projector() {
        let dir = root();
        let created =
            create(dir.path(), "prepare the release", "installation-1").expect("create work");
        let work_id = created["work_id"].as_str().unwrap();
        let thread_id = created["primary_thread_id"].as_str().unwrap();
        let service = service(dir.path()).unwrap();
        let mut request = heiwa_session::operator::StartTurnRequest::auto("work-show", "ship it");
        request.work_id = Some(work_id.to_string());
        let submission = service.start_turn(thread_id, request).unwrap();
        service
            .append_event(scoped_event(
                work_id,
                thread_id,
                &submission.turn_id,
                Some("call-1"),
                OperatorEventType::ToolCallStarted,
                json!({"name": "fs.write", "arguments": {"hidden": true}}),
            ))
            .unwrap();
        service
            .append_event(scoped_event(
                work_id,
                thread_id,
                &submission.turn_id,
                Some("call-1"),
                OperatorEventType::ToolCallCompleted,
                json!({
                    "name": "fs.write",
                    "status": "success",
                    "output": "hidden",
                    "receipt_id": "receipt-action-1"
                }),
            ))
            .unwrap();
        service
            .append_event(scoped_event(
                work_id,
                thread_id,
                &submission.turn_id,
                None,
                OperatorEventType::ReceiptLinked,
                json!({"kind": "operator_turn", "receipt_ref": "receipt-turn-1"}),
            ))
            .unwrap();
        service
            .append_event(scoped_event(
                work_id,
                thread_id,
                &submission.turn_id,
                None,
                OperatorEventType::TurnCompleted,
                json!({"trace": {"hidden": true}}),
            ))
            .unwrap();

        let snapshot = session(dir.path(), work_id, "command-test").unwrap();
        assert_eq!(snapshot.work_id, work_id);
        assert_eq!(
            snapshot.collections["threads"][thread_id]["status"],
            "completed"
        );
        assert_eq!(
            snapshot.collections["actions"]["call-1"]["status"],
            "success"
        );
        assert_eq!(snapshot.collections["receipts"].len(), 1);
        assert!(snapshot.operator_cursor.is_some());
        let encoded = serde_json::to_string(&snapshot).unwrap();
        assert!(!encoded.contains("hidden"), "{encoded}");
    }

    #[test]
    fn a_damaged_work_event_is_counted_rather_than_hidden() {
        use heiwa_work::{work_linked_event, WorkLinkOrigin};

        let dir = root();
        create(dir.path(), "prepare the release", "installation-1").expect("create work");

        // A link naming a Work that was never created: real damage. Built
        // here through the same public API the command uses, so no test-only
        // helper has to exist in the production module.
        let service = service(dir.path()).expect("service");
        service.ensure_thread("thread-orphan").expect("thread");
        service
            .append_event(work_linked_event(
                &WorkId::parse("work-missing").expect("id"),
                "thread-orphan",
                WorkLinkOrigin::Minted,
                "2026-08-22T00:01:00Z",
                || "evt-orphan".to_string(),
            ))
            .expect("append orphan link");

        let summary = summarize(dir.path()).expect("summarize");
        assert_eq!(
            summary["skipped_events"], 1,
            "damage found while folding must reach the surface: {summary}"
        );
    }

    #[test]
    fn summarizing_related_threads_preserves_global_event_order() {
        use heiwa_work::{work_linked_event, WorkLinkOrigin};

        let dir = root();
        let service = service(dir.path()).expect("service");
        service.ensure_thread("thread-z-primary").expect("primary");
        service.ensure_thread("thread-a-related").expect("related");
        let work_id = WorkId::parse("work-ordered").expect("work id");
        service
            .append_event(work_created_event(
                &work_id,
                "thread-z-primary",
                "preserve event order",
                "installation-1",
                "2026-08-24T00:00:00Z",
                || "evt-created".to_string(),
            ))
            .expect("work created");
        service
            .append_event(work_linked_event(
                &work_id,
                "thread-a-related",
                WorkLinkOrigin::Adopted,
                "2026-08-24T00:01:00Z",
                || "evt-linked".to_string(),
            ))
            .expect("work linked");

        let summary = summarize(dir.path()).expect("summarize");
        assert_eq!(summary["skipped_events"], 0, "{summary}");
        assert_eq!(
            summary["work"][0]["related_thread_ids"],
            serde_json::json!(["thread-a-related"]),
            "{summary}"
        );
    }

    fn page_lines(step: WatchStep) -> (Vec<Value>, Option<String>) {
        match step {
            WatchStep::Page { lines, cursor } => (lines, cursor),
            WatchStep::Resync { reason } => panic!("unexpected resync: {reason}"),
        }
    }

    fn found(root: &Path, created: &Value) -> Work {
        find(root, created["work_id"].as_str().expect("work id"))
            .expect("find")
            .expect("work exists")
    }

    #[test]
    fn watching_reads_only_this_works_events_and_resumes_from_its_cursor() {
        let dir = root();
        let first = create(dir.path(), "first", "installation-1").expect("create first");
        let second = create(dir.path(), "second", "installation-1").expect("create second");
        let mut scope = WatchScope::new(&found(dir.path(), &first));

        let (lines, cursor) =
            page_lines(watch_page(dir.path(), &mut scope, None, 256).expect("page"));
        assert!(
            lines.iter().any(|line| {
                line["event"]["event_type"] == "work_created" && line["scope"] == "work"
            }),
            "{lines:?}"
        );
        assert!(
            lines.iter().all(|line| {
                line["event"]["work_id"] != second["work_id"]
                    && line["event"]["thread_id"] != second["primary_thread_id"]
            }),
            "the other Work leaked into this stream: {lines:?}"
        );
        assert!(lines
            .iter()
            .all(|line| line["schema"] == "heiwa.cli.stream/v1" && line["type"] == "event"));

        let (resumed, resumed_cursor) = page_lines(
            watch_page(dir.path(), &mut scope, cursor.as_deref(), 256).expect("resume"),
        );
        assert!(resumed.is_empty(), "{resumed:?}");
        assert_eq!(resumed_cursor, cursor, "an idle page keeps its place");
    }

    #[test]
    fn watching_reports_activity_appended_after_the_cursor() {
        let dir = root();
        let created = create(dir.path(), "watched", "installation-1").expect("create");
        let work_id = created["work_id"].as_str().expect("id").to_string();
        let thread_id = created["primary_thread_id"]
            .as_str()
            .expect("thread")
            .to_string();
        let mut scope = WatchScope::new(&found(dir.path(), &created));
        let (_, caught_up) =
            page_lines(watch_page(dir.path(), &mut scope, None, 256).expect("page"));

        let service = service(dir.path()).expect("service");
        let mut request =
            heiwa_session::operator::StartTurnRequest::auto("work-watch", "keep going");
        request.work_id = Some(work_id.clone());
        service.start_turn(&thread_id, request).expect("start turn");

        let (lines, cursor) = page_lines(
            watch_page(dir.path(), &mut scope, caught_up.as_deref(), 256).expect("next page"),
        );
        assert!(
            lines
                .iter()
                .any(|line| line["event"]["event_type"] == "turn_started"),
            "{lines:?}"
        );
        assert!(lines
            .iter()
            .all(|line| line["event"]["thread_id"] == thread_id.as_str()));
        assert_ne!(cursor, caught_up);
    }

    #[test]
    fn a_malformed_cursor_is_a_usage_error() {
        let dir = root();
        let created = create(dir.path(), "watched", "installation-1").expect("create");
        let mut scope = WatchScope::new(&found(dir.path(), &created));
        let error = match watch_page(dir.path(), &mut scope, Some("not-a-cursor"), 256) {
            Ok(_) => panic!("a malformed cursor must be refused"),
            Err(error) => error,
        };
        assert_eq!(
            crate::output::classify(&error).code,
            crate::output::ErrorCode::Usage,
            "{error:#}"
        );
    }

    #[test]
    fn a_cursor_from_another_stream_asks_for_a_resync() {
        let here = root();
        let elsewhere = root();
        let created = create(here.path(), "watched", "installation-1").expect("create");
        create(elsewhere.path(), "unrelated", "installation-1").expect("create elsewhere");
        let foreign = OperatorJournal::new(elsewhere.path().to_path_buf())
            .expect("journal")
            .read_after(None, 256)
            .expect("read")
            .next_cursor
            .expect("a cursor from the other stream");

        let mut scope = WatchScope::new(&found(here.path(), &created));
        match watch_page(here.path(), &mut scope, Some(&foreign), 256).expect("step") {
            WatchStep::Resync { reason } => assert!(reason.contains("fingerprint"), "{reason}"),
            WatchStep::Page { lines, .. } => {
                panic!("a cursor from another lineage must resync, got {lines:?}")
            }
        }
    }

    #[test]
    fn another_works_events_in_a_shared_thread_are_not_this_works_context() {
        let dir = root();
        let created = create(dir.path(), "watched", "installation-1").expect("create");
        let work_id = created["work_id"].as_str().expect("id").to_string();
        let shared = created["primary_thread_id"]
            .as_str()
            .expect("thread")
            .to_string();
        let mut scope = WatchScope::new(&found(dir.path(), &created));

        let other = scoped_event(
            "work-b",
            &shared,
            "turn-b",
            None,
            OperatorEventType::TurnStarted,
            json!({}),
        );
        assert_eq!(
            scope.admit(&other),
            None,
            "another Work's explicitly scoped events are not this Work's context"
        );

        let mut legacy = scoped_event(
            &work_id,
            &shared,
            "turn-legacy",
            None,
            OperatorEventType::TurnStarted,
            json!({}),
        );
        legacy.work_id = None;
        assert_eq!(
            scope.admit(&legacy),
            Some("thread"),
            "unscoped turns in this Work's thread stay visible"
        );
    }

    #[test]
    fn a_thread_joins_the_watch_once_the_work_acts_in_it() {
        let dir = root();
        let created = create(dir.path(), "watched", "installation-1").expect("create");
        let work_id = created["work_id"].as_str().expect("id").to_string();
        let mut scope = WatchScope::new(&found(dir.path(), &created));
        let late = |turn: &str, scoped: bool| {
            let mut event = scoped_event(
                &work_id,
                "thread-late",
                turn,
                None,
                OperatorEventType::TurnStarted,
                json!({}),
            );
            if !scoped {
                event.work_id = None;
            }
            event
        };

        assert_eq!(scope.admit(&late("turn-1", false)), None, "not yet watched");
        assert_eq!(scope.admit(&late("turn-2", true)), Some("work"));
        assert_eq!(
            scope.admit(&late("turn-3", false)),
            Some("thread"),
            "the thread joined once the Work acted in it"
        );
    }
}
