//! The `heiwa.cli/v1` contract, driven through the built binary with an
//! isolated HOME, so no test reads or writes durable operator state.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

mod cli_v1;

fn heiwa(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", home)
        .env("HEIWA_HOME", home.join(".heiwa"))
        .env("HEIWA_DISABLE_KEYCHAIN", "1")
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C")
        .current_dir(home)
        .args(args)
        .output()
        .expect("run heiwa")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn an_unknown_command_is_a_usage_error_on_both_streams() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["definitely-not-a-command", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let error: Value = cli_v1::error(&output.stdout);
    assert_eq!(error["code"], "usage");
    assert!(
        stderr(&output).contains("unknown command: definitely-not-a-command"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn help_json_is_a_v1_catalog_that_includes_every_command() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["help", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let data = cli_v1::data(&output.stdout);
    let names: Vec<&str> = data["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .map(|command| command["name"].as_str().expect("name"))
        .collect();
    for expected in ["calendar", "connect", "approvals", "work", "version"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn human_help_lists_commands_the_old_help_omitted() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["help"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("calendar status|sync|hold|plan"), "{text}");
    assert!(text.contains("connect <connector>"), "{text}");
}

#[test]
fn work_list_is_an_envelope_that_suggests_creating_work() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["work", "list", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(cli_v1::data(&output.stdout)["work"], serde_json::json!([]));
    assert!(
        cli_v1::next(&output.stdout)[0].starts_with("heiwa work create"),
        "{:?}",
        cli_v1::next(&output.stdout)
    );
}

#[test]
fn showing_unknown_work_is_not_found() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["work", "show", "work-missing", "--json"]);
    assert_eq!(output.status.code(), Some(10), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "not_found");
}

#[test]
fn an_unknown_work_subcommand_is_a_usage_error() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["work", "frobnicate", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "usage");
}

#[test]
fn human_work_watch_prints_events_and_a_reusable_resume_command() {
    let home = home_with_identity();
    let work_id = create_work(home.path(), "human watcher");
    let output = heiwa(home.path(), &["work", "watch", &work_id, "--once"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = String::from_utf8(output.stdout).expect("text output");
    assert!(text.contains("work_created"), "{text}");
    let prefix = format!("caught up; resume with: heiwa work watch {work_id} --since ");
    let cursor = text
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .expect("resume hint");
    let resumed = heiwa(
        home.path(),
        &["work", "watch", &work_id, "--since", cursor, "--once"],
    );
    assert!(resumed.status.success(), "{}", stderr(&resumed));
    let resumed_text = String::from_utf8(resumed.stdout).expect("text output");
    assert_eq!(resumed_text.trim(), format!("{prefix}{cursor}"));
}

fn home_with_identity() -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("home");
    heiwa_identity::establish_in(
        &home.path().join(".heiwa"),
        "Test operator",
        "2026-09-27T00:00:00Z",
        || "install-test".to_string(),
    )
    .expect("identity");
    home
}

fn create_work(home: &Path, intent: &str) -> String {
    let created = heiwa(home, &["work", "create", intent, "--json"]);
    assert!(created.status.success(), "{}", stderr(&created));
    cli_v1::data(&created.stdout)["work_id"]
        .as_str()
        .expect("work id")
        .to_string()
}

fn ndjson(stdout: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each stream line is JSON"))
        .collect()
}

#[test]
fn watching_once_streams_events_then_an_end_line_to_resume_from() {
    let home = home_with_identity();
    let work_id = create_work(home.path(), "watch me");

    let watched = heiwa(
        home.path(),
        &["work", "watch", &work_id, "--once", "--json"],
    );
    assert!(watched.status.success(), "{}", stderr(&watched));
    let lines = ndjson(&watched.stdout);
    assert!(
        lines
            .iter()
            .all(|line| line["schema"] == "heiwa.cli.stream/v1"),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line["type"] == "event" && line["event"]["event_type"] == "work_created"),
        "{lines:?}"
    );
    let end = lines.last().expect("an end line");
    assert_eq!(end["type"], "end");
    let cursor = end["cursor"].as_str().expect("resume cursor").to_string();

    let resumed = heiwa(
        home.path(),
        &[
            "work", "watch", &work_id, "--since", &cursor, "--once", "--json",
        ],
    );
    assert!(resumed.status.success(), "{}", stderr(&resumed));
    let resumed_lines = ndjson(&resumed.stdout);
    assert_eq!(
        resumed_lines.len(),
        1,
        "only the end line: {resumed_lines:?}"
    );
    assert_eq!(resumed_lines[0]["type"], "end");
    assert_eq!(resumed_lines[0]["cursor"], cursor.as_str());
}

#[test]
fn a_cursor_from_another_stream_resyncs_instead_of_failing() {
    let here = home_with_identity();
    let elsewhere = home_with_identity();
    let work_id = create_work(here.path(), "watched here");
    let other_id = create_work(elsewhere.path(), "watched elsewhere");
    let foreign = ndjson(
        &heiwa(
            elsewhere.path(),
            &["work", "watch", &other_id, "--once", "--json"],
        )
        .stdout,
    );
    let foreign_cursor = foreign.last().expect("end")["cursor"]
        .as_str()
        .expect("cursor")
        .to_string();

    let output = heiwa(
        here.path(),
        &[
            "work",
            "watch",
            &work_id,
            "--since",
            &foreign_cursor,
            "--once",
            "--json",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let lines = ndjson(&output.stdout);
    assert_eq!(lines[0]["type"], "resync", "{lines:?}");
    assert!(lines[0]["cursor"].is_null(), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|line| line["type"] == "event" && line["event"]["event_type"] == "work_created"),
        "the Work replays after a resync: {lines:?}"
    );
    assert_eq!(lines.last().expect("end")["type"], "end");
}

#[test]
fn a_malformed_cursor_is_a_usage_error() {
    let home = home_with_identity();
    let work_id = create_work(home.path(), "watched");
    let output = heiwa(
        home.path(),
        &[
            "work",
            "watch",
            &work_id,
            "--since",
            "not-a-cursor",
            "--once",
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "usage");
}

#[test]
fn a_closed_pipe_ends_the_stream_without_a_panic() {
    let home = home_with_identity();
    let work_id = create_work(home.path(), "watched");
    let mut child = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", home.path())
        .env("HEIWA_HOME", home.path().join(".heiwa"))
        .env("HEIWA_DISABLE_KEYCHAIN", "1")
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C")
        .args(["work", "watch", &work_id, "--once", "--json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn heiwa");
    drop(child.stdout.take());
    let output = child.wait_with_output().expect("wait");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert!(output.status.success(), "{stderr}");
}

#[test]
fn help_survives_a_closed_pipe() {
    let home = tempfile::tempdir().expect("home");
    let mut child = Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", home.path())
        .env("HEIWA_HOME", home.path().join(".heiwa"))
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C")
        .arg("help")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn heiwa");
    drop(child.stdout.take());
    let output = child.wait_with_output().expect("wait");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert!(output.status.success(), "{stderr}");
}

#[test]
fn a_since_flag_without_a_value_is_a_usage_error_not_a_replay() {
    let home = home_with_identity();
    let work_id = create_work(home.path(), "watched");
    let output = heiwa(
        home.path(),
        &["work", "watch", &work_id, "--since", "--once", "--json"],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "usage");
}

#[test]
fn a_surface_flag_without_a_value_is_a_usage_error() {
    let home = home_with_identity();
    let work_id = create_work(home.path(), "shown");
    let output = heiwa(
        home.path(),
        &["work", "show", &work_id, "--surface", "--json"],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "usage");
}

#[test]
fn a_replaced_stream_without_the_work_is_not_found_rather_than_stale() {
    let here = home_with_identity();
    let elsewhere = home_with_identity();
    let work_id = create_work(here.path(), "watched here");
    create_work(elsewhere.path(), "the only Work elsewhere");
    let cursor = ndjson(
        &heiwa(
            here.path(),
            &["work", "watch", &work_id, "--once", "--json"],
        )
        .stdout,
    )
    .last()
    .expect("end")["cursor"]
        .as_str()
        .expect("cursor")
        .to_string();

    let stream = |home: &Path| home.join(".heiwa/evidence/operator_events.jsonl");
    std::fs::copy(stream(elsewhere.path()), stream(here.path())).expect("replace the stream");

    let output = heiwa(
        here.path(),
        &[
            "work", "watch", &work_id, "--since", &cursor, "--once", "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(10), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "not_found");
}

#[test]
fn approvals_list_is_an_envelope() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["approvals", "list", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        cli_v1::data(&output.stdout)["pending_summary"],
        serde_json::json!([])
    );
}

#[test]
fn showing_an_unknown_approval_is_not_found() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["approvals", "show", "req_missing", "--json"]);
    assert_eq!(output.status.code(), Some(10), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "not_found");
}

#[test]
fn an_approval_id_cannot_walk_out_of_the_requests_directory() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(
        home.path(),
        &["approvals", "show", "../../secrets", "--json"],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "usage");
}

#[test]
fn deciding_without_an_outcome_is_a_usage_error() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["approvals", "decide", "req_any", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(cli_v1::error(&output.stdout)["code"], "usage");
}

#[test]
fn approval_hints_come_from_validated_file_stems_never_request_content() {
    let home = tempfile::tempdir().expect("home");
    // Ask the binary where it reads requests instead of assuming a layout.
    let listed = heiwa(home.path(), &["approvals", "list", "--json"]);
    let requests = std::path::PathBuf::from(
        cli_v1::data(&listed.stdout)["requests_dir"]
            .as_str()
            .expect("requests_dir"),
    );
    std::fs::create_dir_all(&requests).expect("requests dir");
    let canary = home.path().join("pwned");
    let hostile = format!("x; touch {}", canary.display());
    let write = |name: &str, body: serde_json::Value| {
        std::fs::write(requests.join(name), body.to_string()).expect("request file");
    };
    // A contained worker controls request content, never the command we suggest.
    write(
        "req_evil.json",
        serde_json::json!({"id": hostile, "action": "write-file"}),
    );
    write(
        "req_ok.json",
        serde_json::json!({"request_id": "req_ok", "action": "write-file"}),
    );
    write("bad id.json", serde_json::json!({"action": "write-file"}));
    write("--json.json", serde_json::json!({"action": "write-file"}));

    let output = heiwa(home.path(), &["approvals", "list", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let mut next = cli_v1::next(&output.stdout);
    next.sort();
    assert_eq!(
        next,
        vec![
            "heiwa approvals show req_evil".to_string(),
            "heiwa approvals show req_ok".to_string()
        ],
        "hints use validated stems only"
    );
    let summaries = cli_v1::data(&output.stdout)["pending_summary"].to_string();
    assert!(
        summaries.contains(&hostile),
        "content stays visible as data: {summaries}"
    );
    assert!(!canary.exists());
}

#[test]
fn work_shadow_reports_through_the_envelope_with_nothing_recorded_by_default() {
    let home = tempfile::tempdir().expect("home");
    let output = heiwa(home.path(), &["work", "shadow", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let data = cli_v1::data(&output.stdout);
    // System 1 is shadow-only and off unless enabled: a fresh profile has
    // recorded nothing.
    assert_eq!(data["records"], 0, "{data}");
    assert!(data["coverage"].is_object(), "{data}");
}
