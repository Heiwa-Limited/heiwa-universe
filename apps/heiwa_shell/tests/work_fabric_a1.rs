//! Work Fabric A1 acceptance through the built `heiwa` binary.
//!
//! A1's claim is user-level: surfaces agree on one Work, and a restart leaves
//! truthful run state without repeating effects. So this drives the binary as
//! a process — including killing the process that supervises a worker — in an
//! isolated runtime, instead of calling library functions.

use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

mod work_support;
use work_support::{heiwa, heiwa_command, prepared_work, successful, PreparedWork};

/// A spawned process that is killed and reaped however the test ends.
struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A provider pid that must not outlive the test, even when an assertion fails.
struct OrphanGuard(u64);

impl Drop for OrphanGuard {
    fn drop(&mut self) {
        if process_alive(self.0) {
            kill_and_wait_gone(self.0);
        }
    }
}

fn json(output: std::process::Output, command: &str) -> Value {
    let output = successful(output, command);
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{command} JSON: {error}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn surfaces(work: &PreparedWork) -> Vec<Value> {
    let rendered = json(
        heiwa(
            &work.runtime_root,
            &work.repo,
            &["work", "show", &work.work_id, "--surface", "all"],
        ),
        "work show --surface all",
    );
    let surfaces = rendered["surfaces"].as_array().expect("surfaces").clone();
    let names: Vec<_> = surfaces
        .iter()
        .map(|view| view["surface"].clone())
        .collect();
    assert_eq!(names, vec!["home", "work", "agent"]);
    for view in &surfaces {
        assert_eq!(
            view["identity"], surfaces[0]["identity"],
            "Home, Work, and Agent must report one Work, revision, epoch, and cursor"
        );
        assert_eq!(view["identity"]["work_id"], work.work_id.as_str());
    }
    surfaces
}

/// The single run row, identical on every surface that shows runs.
fn only_run(surfaces: &[Value]) -> Value {
    let agent = surfaces
        .iter()
        .find(|view| view["surface"] == "agent")
        .expect("agent");
    let runs = agent["collections"]["runs"].as_object().expect("runs");
    assert_eq!(runs.len(), 1, "exactly one run: nothing was relaunched");
    for view in surfaces {
        if let Some(rows) = view["collections"].get("runs") {
            assert_eq!(
                rows, &agent["collections"]["runs"],
                "runs agree across surfaces"
            );
        }
    }
    runs.values().next().expect("run").clone()
}

fn recover(work: &PreparedWork) -> std::process::Output {
    heiwa(
        &work.runtime_root,
        &work.repo,
        &["work", "recover", "--json"],
    )
}

fn process_alive(pid: u64) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn kill_and_wait_gone(pid: u64) {
    let _ = std::process::Command::new("/bin/kill")
        .args(["-9", &pid.to_string()])
        .stderr(Stdio::null())
        .status();
    let deadline = Instant::now() + Duration::from_secs(10);
    while process_alive(pid) {
        assert!(Instant::now() < deadline, "pid {pid} did not exit");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn worktree_listing(worktree: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(worktree)
        .expect("worktree")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// Start a worker whose provider records one effect and then keeps running,
/// and wait until the journal shows it live.
fn start_long_worker(work: &PreparedWork) -> (Reaped, OrphanGuard) {
    let owner = Reaped(
        heiwa_command(
            &work.runtime_root,
            &work.repo,
            &[
                "work",
                "run",
                &work.work_id,
                "--json",
                "--",
                "/bin/sh",
                "-c",
                "echo started >> effects.log; exec /bin/sleep 120",
            ],
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start heiwa work run"),
    );

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let shown = json(
            heiwa(
                &work.runtime_root,
                &work.repo,
                &["work", "show", &work.work_id, "--json"],
            ),
            "work show --json",
        );
        let live = shown["collections"]["runs"]
            .as_object()
            .and_then(|runs| runs.values().next())
            .filter(|run| run["worker_state"] == "live")
            .and_then(|run| run["pid"].as_u64());
        if let (Some(pid), true) = (live, work.worktree.join("effects.log").exists()) {
            return (owner, OrphanGuard(pid));
        }
        assert!(
            Instant::now() < deadline,
            "worker never became live: {shown}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a1_surfaces_agree_and_a_completed_run_needs_no_recovery() {
    let work = prepared_work("A1 completed run");
    json(
        heiwa(
            &work.runtime_root,
            &work.repo,
            &[
                "work",
                "run",
                &work.work_id,
                "--json",
                "--",
                "/bin/sh",
                "-c",
                "echo made > made.txt",
            ],
        ),
        "work run",
    );
    assert!(
        work.worktree.join("made.txt").exists(),
        "the effect lands in the worktree"
    );
    let source_status = std::process::Command::new("git")
        .current_dir(&work.repo)
        .args(["status", "--porcelain"])
        .output()
        .expect("git status");
    assert!(
        source_status.stdout.is_empty(),
        "the source repository stays clean"
    );

    let run = only_run(&surfaces(&work));
    assert_eq!(run["worker_state"], "exited");
    assert_eq!(run["exit_code"], 0);
    assert!(run["supervision"].is_null());

    for pass in ["first", "second"] {
        let report = json(recover(&work), "work recover");
        assert_eq!(
            report["runs_marked_stale"], 0,
            "{pass} recovery of a finished run"
        );
    }
    assert_eq!(only_run(&surfaces(&work))["worker_state"], "exited");
}

#[test]
fn a1_restart_recovery_records_a_surviving_child_once_and_repeats_no_effect() {
    let work = prepared_work("A1 interrupted owner");
    let (mut owner, orphan) = start_long_worker(&work);
    let pid = orphan.0;

    // While the owner lives, its run is supervised and must not be marked.
    let refused = recover(&work);
    assert!(
        !refused.status.success(),
        "recovery must refuse while the owner holds activity"
    );
    assert!(String::from_utf8_lossy(&refused.stderr).contains("still supervised"));
    assert_eq!(only_run(&surfaces(&work))["worker_state"], "live");

    // Kill only the supervising `heiwa`; its provider child keeps running.
    owner.0.kill().expect("kill owner");
    owner.0.wait().expect("reap owner");
    assert!(process_alive(pid), "the provider outlived its owner");
    let before = worktree_listing(&work.worktree);

    let report = json(recover(&work), "work recover");
    assert_eq!(report["runs_marked_stale"], 1);
    assert_eq!(report["runs"][0]["process"], "alive");
    assert_eq!(report["runs"][0]["pid"], pid);
    let again = json(recover(&work), "work recover (repeat)");
    assert_eq!(again["runs_marked_stale"], 0, "recovery is idempotent");

    // Each read below is a fresh process over the durable journal.
    let run = only_run(&surfaces(&work));
    assert_eq!(run["worker_state"], "stale");
    assert_eq!(run["supervision"]["process"], "alive");
    assert!(run["exit_code"].is_null(), "stale is not an ending");
    assert!(run["ended_at"].is_null());
    let shown = successful(
        heiwa(
            &work.runtime_root,
            &work.repo,
            &["work", "show", &work.work_id],
        ),
        "work show",
    );
    assert!(
        String::from_utf8_lossy(&shown.stdout).contains("process still running unsupervised"),
        "presentation must not imply stale means stopped"
    );

    assert!(process_alive(pid), "recovery never stops the process");
    assert_eq!(
        worktree_listing(&work.worktree),
        before,
        "recovery touches no files"
    );
    let effects = std::fs::read_to_string(work.worktree.join("effects.log")).expect("effects");
    assert_eq!(
        effects.lines().count(),
        1,
        "the provider was never relaunched"
    );

    kill_and_wait_gone(pid);
}

#[test]
fn a1_restart_recovery_records_a_dead_owner_and_child_as_gone() {
    let work = prepared_work("A1 dead worker");
    let (mut owner, orphan) = start_long_worker(&work);
    let pid = orphan.0;
    owner.0.kill().expect("kill owner");
    owner.0.wait().expect("reap owner");
    kill_and_wait_gone(pid);

    let report = json(recover(&work), "work recover");
    assert_eq!(report["runs_marked_stale"], 1);
    assert_eq!(report["runs"][0]["process"], "gone");

    let run = only_run(&surfaces(&work));
    assert_eq!(run["worker_state"], "stale");
    assert_eq!(run["supervision"]["process"], "gone");
    let shown = successful(
        heiwa(
            &work.runtime_root,
            &work.repo,
            &["work", "show", &work.work_id],
        ),
        "work show",
    );
    assert!(String::from_utf8_lossy(&shown.stdout).contains("process no longer running"));

    // Recording the loss does not free the Work for a silent relaunch.
    let effects = std::fs::read_to_string(work.worktree.join("effects.log")).expect("effects");
    assert_eq!(effects.lines().count(), 1);
}

fn free_loopback_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

#[test]
fn a1_app_restart_recovers_an_orphaned_run_before_it_serves() {
    let work = prepared_work("A1 app restart");
    let (mut owner, orphan) = start_long_worker(&work);
    let pid = orphan.0;
    owner.0.kill().expect("kill owner");
    owner.0.wait().expect("reap owner");
    assert!(process_alive(pid));

    // The app runtime binds its port only after exclusive restart recovery, so
    // reachability is the moment recovery has already been recorded.
    let port = free_loopback_port();
    let app_log = work.runtime_root.join("app-start.log");
    let mut app = Reaped(
        heiwa_command(
            &work.runtime_root,
            &work.repo,
            &["app", "start", "--port", &port.to_string(), "--no-open"],
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&app_log).expect("app log"))
        .spawn()
        .expect("start app runtime"),
    );

    let deadline = Instant::now() + Duration::from_secs(60);
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        if let Some(status) = app.0.try_wait().expect("poll app") {
            panic!(
                "app runtime exited before serving ({status}): {}",
                std::fs::read_to_string(&app_log).unwrap_or_default()
            );
        }
        assert!(Instant::now() < deadline, "app runtime never served");
        std::thread::sleep(Duration::from_millis(100));
    }

    let run = only_run(&surfaces(&work));
    drop(app);

    assert_eq!(
        run["worker_state"], "stale",
        "restart recorded the lost supervisor"
    );
    assert_eq!(run["supervision"]["process"], "alive");
    assert_eq!(run["supervision"]["pid"], pid);
    assert!(process_alive(pid), "the restart did not stop the orphan");
    let effects = std::fs::read_to_string(work.worktree.join("effects.log")).expect("effects");
    assert_eq!(
        effects.lines().count(),
        1,
        "the restart did not relaunch the provider"
    );

    // A second restart has nothing left to record.
    let report = json(recover(&work), "work recover after app restart");
    assert_eq!(report["runs_marked_stale"], 0);

    kill_and_wait_gone(pid);
}

/// The operator stream file under an isolated runtime root.
fn operator_stream(root: &Path) -> std::path::PathBuf {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .file_name()
                .is_some_and(|name| name == "operator_events.jsonl")
            {
                return path;
            }
        }
    }
    panic!("no operator stream under {}", root.display());
}

#[test]
fn a1_recovery_never_interprets_a_worker_row_this_build_cannot_admit() {
    // Review reproduction: a schema-999 `worker_launched` under a valid Work,
    // with no real provider process, must not become a current-schema marker.
    let work = prepared_work("A1 future schema");
    let stream = operator_stream(&work.runtime_root);
    let original = std::fs::read_to_string(&stream).expect("stream");
    let last: Value =
        serde_json::from_str(original.lines().last().expect("an envelope")).expect("envelope");
    let mut future = last.clone();
    let record = future["record"].as_object_mut().expect("record");
    let thread_id = record["thread_id"].clone();
    record.insert("schema_version".into(), 999.into());
    record.insert("event_id".into(), "future-launch-event".into());
    record.insert("event_type".into(), "worker_launched".into());
    record.insert("work_id".into(), work.work_id.as_str().into());
    record.insert("thread_id".into(), thread_id);
    record.insert("run_id".into(), "future-run".into());
    record.insert("turn_id".into(), Value::Null);
    record.insert("call_id".into(), Value::Null);
    record.insert(
        "actor".into(),
        serde_json::json!({"kind": "worker", "id": "future-worker"}),
    );
    record.insert(
        "payload".into(),
        serde_json::json!({
            "worker_id": "future-worker", "provider": "fixture", "provider_session_ref": null,
            "executable_path": "/bin/true", "executable_sha256": "a".repeat(64),
            "cwd": "/tmp", "repo_root": "/tmp", "branch": "fixture",
            "base_commit": "b".repeat(40), "lease_id": "fixture-lease",
            "installation_id": "install-test",
        }),
    );
    let mut appended = original.clone();
    appended.push_str(&serde_json::to_string(&future).expect("encode"));
    appended.push('\n');
    std::fs::write(&stream, &appended).expect("append future row");

    let report = json(recover(&work), "work recover");
    assert_eq!(report["runs_marked_stale"], 0, "{report}");
    let unadmitted = report["unadmitted_worker_events"]
        .as_array()
        .expect("unadmitted");
    assert!(
        unadmitted
            .iter()
            .any(|doubt| doubt["run_id"] == "future-run" && doubt["reason"] == "unsupported_schema"),
        "the uninterpreted row is reported: {report}"
    );
    assert_eq!(
        std::fs::read_to_string(&stream).expect("stream after"),
        appended,
        "recovery preserved the unsupported evidence and appended nothing"
    );
}
