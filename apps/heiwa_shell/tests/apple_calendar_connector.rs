//! Mac-first L3 connector acceptance against a hermetic `osascript` fixture.
//!
//! The real binary owns staging, approval, state, receipts, and journal replay.
//! Only Calendar.app itself is replaced so CI never touches a user's calendar.

#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

fn fixture_osascript(root: &Path) -> PathBuf {
    let path = root.join("fixture-osascript");
    fs::write(
        &path,
        r#"#!/bin/sh
set -eu
mode="$5"
printf '%s\n' "$mode" >> "$HEIWA_APPLE_CALENDAR_FIXTURE_LOG"
case "$mode" in
  list)
    printf '%s\n' '[{"name":"Calendar","writable":true},{"name":"Birthdays","writable":false}]'
    ;;
  create)
    if [ -e "$HEIWA_APPLE_CALENDAR_FIXTURE_EVENT" ]; then
      created=false
    else
      : > "$HEIWA_APPLE_CALENDAR_FIXTURE_EVENT"
      created=true
    fi
    printf '%s\n' "{\"calendar\":\"Calendar\",\"external_id\":\"fixture-event-123\",\"marker\":\"heiwa://calendar/holds/fixture\",\"title\":\"call mom\",\"start\":\"2026-06-19T15:00:00-07:00\",\"end\":\"2026-06-19T15:30:00-07:00\",\"created\":$created}"
    ;;
  *)
    printf '%s\n' "unknown fixture mode: $mode" >&2
    exit 2
    ;;
esac
"#,
    )
    .expect("write osascript fixture");
    let mut permissions = fs::metadata(&path).expect("fixture metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("make fixture executable");
    path
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn available_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("bind ephemeral port")
        .local_addr()
        .expect("ephemeral address")
        .port()
}

fn wait_for_runtime(port: u16) {
    for _ in 0..120 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("temporary Heiwa runtime did not start on port {port}");
}

fn post_json(port: u16, target: &str, body: &serde_json::Value) -> String {
    let body = body.to_string();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect runtime");
    write!(
        stream,
        "POST {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer apple-connector-test-token\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("write request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    response
}

struct Fixture {
    _root: tempfile::TempDir,
    home: PathBuf,
    evidence: PathBuf,
    bridge: PathBuf,
    log: PathBuf,
    event_state: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp fixture root");
        let home = root.path().join("home");
        let evidence = root.path().join("evidence");
        let log = root.path().join("bridge.log");
        let event_state = root.path().join("event-created");
        fs::create_dir_all(&home).expect("create temp home");
        let bridge = fixture_osascript(root.path());
        Self {
            _root: root,
            home,
            evidence,
            bridge,
            log,
            event_state,
        }
    }

    fn heiwa(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_heiwa"));
        command
            .env("HOME", &self.home)
            .env("HEIWA_EVIDENCE_DIR", &self.evidence)
            .env("HEIWA_APPLE_CALENDAR_OSASCRIPT", &self.bridge)
            .env("HEIWA_APPLE_CALENDAR_FIXTURE_LOG", &self.log)
            .env("HEIWA_APPLE_CALENDAR_FIXTURE_EVENT", &self.event_state)
            .env_remove("HEIWA_HOME")
            .env_remove("HEIWA_STATE_DIR");
        command
    }
}

#[test]
fn lists_writable_apple_calendar_resources_without_writing_heiwa_state() {
    let fixture = Fixture::new();
    let output = fixture
        .heiwa()
        .args(["calendar", "calendars", "--source", "apple", "--json"])
        .output()
        .expect("calendar resource listing runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("calendar resources JSON");
    assert_eq!(payload["source"], "apple_calendar");
    assert_eq!(payload["status"], "ready");
    assert_eq!(payload["calendars"][0]["name"], "Calendar");
    assert_eq!(payload["calendars"][0]["writable"], true);
    assert_eq!(payload["revoke"]["owner"], "macOS");
    assert!(
        !fixture.home.join(".heiwa").exists(),
        "resource discovery must not create Heiwa state"
    );
}

#[test]
fn approval_executes_apple_write_and_replays_connector_receipt() {
    let fixture = Fixture::new();
    let staged = fixture
        .heiwa()
        .args([
            "schedule",
            "call",
            "mom",
            "--at",
            "2026-06-19T15:00",
            "--promote",
            "apple",
            "--calendar",
            "Calendar",
            "--json",
        ])
        .output()
        .expect("schedule staging runs");
    assert!(
        staged.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&staged.stderr)
    );
    let staged: serde_json::Value = serde_json::from_slice(&staged.stdout).expect("staging JSON");
    let request_id = staged["approval_request"]["request_id"]
        .as_str()
        .expect("request id");
    let hold_id = staged["hold"]["id"].as_str().expect("hold id");
    let work_id = staged["hold"]["work_id"].as_str().expect("work id");

    assert_eq!(staged["approval_request"]["work_id"], work_id);
    assert_eq!(staged["approval_request"]["risk_tier"], "T2");
    assert_eq!(
        staged["approval_request"]["intent"]["promotion"]["connector"],
        "apple_calendar"
    );
    assert_eq!(
        staged["approval_request"]["intent"]["promotion"]["calendar"],
        "Calendar"
    );
    assert_eq!(staged["hold"]["external_promotion"], "approval_required");
    assert_eq!(
        fs::read_to_string(&fixture.log).expect("bridge log after staging"),
        "list\n",
        "staging may validate the target but must not create an event"
    );
    assert!(!fixture.event_state.exists());

    let approved = fixture
        .heiwa()
        .args(["approvals", "decide", request_id, "--approve", "--json"])
        .output()
        .expect("approval runs");
    assert!(
        approved.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&approved.stderr)
    );
    let approved: serde_json::Value =
        serde_json::from_slice(&approved.stdout).expect("approval JSON");
    assert_eq!(
        approved["decision"]["applied_effects"][0]["kind"],
        "apple_calendar_create"
    );
    assert_eq!(
        approved["decision"]["applied_effects"][0]["external_event"]["external_id"],
        "fixture-event-123"
    );
    assert!(fixture.event_state.exists());

    let hold_path = fixture
        .home
        .join(".heiwa/state/calendar/holds")
        .join(format!("{hold_id}.json"));
    let hold: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&hold_path).expect("promoted hold is persisted"))
            .expect("promoted hold JSON");
    assert_eq!(hold["status"], "confirmed");
    assert_eq!(hold["work_id"], work_id);
    assert_eq!(hold["external_promotion"], "promoted");
    assert_eq!(hold["external_event"]["external_id"], "fixture-event-123");

    let receipt_path = fixture
        .home
        .join(".heiwa/state/calendar/receipts")
        .join(format!("rcpt-{hold_id}-apple-create.json"));
    let receipt: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&receipt_path).expect("connector receipt file"))
            .expect("connector receipt JSON");
    assert_eq!(receipt["schema_version"], "heiwa_connector_receipt_v1");
    assert_eq!(receipt["work_id"], work_id);
    assert_eq!(receipt["approval_id"], request_id);
    assert_eq!(receipt["connector"], "apple_calendar");
    assert_eq!(receipt["external_id"], "fixture-event-123");

    let replay = heiwa_evidence::read_stream(&fixture.evidence, "connector_receipts")
        .expect("connector receipt journal replays");
    assert_eq!(replay.skipped_lines, 0);
    assert_eq!(replay.events.len(), 1);
    assert_eq!(replay.events[0].record["receipt_id"], receipt["receipt_id"]);
    assert_eq!(replay.events[0].record["work_id"], work_id);

    let retried = fixture
        .heiwa()
        .args(["approvals", "decide", request_id, "--approve", "--json"])
        .output()
        .expect("approval retry runs");
    assert!(
        retried.status.success(),
        "retry stderr: {}",
        String::from_utf8_lossy(&retried.stderr)
    );
    let replay = heiwa_evidence::read_stream(&fixture.evidence, "connector_receipts")
        .expect("connector receipt journal replays after retry");
    assert_eq!(
        replay.events.len(),
        1,
        "stable receipt id must not append duplicate evidence"
    );
    let receipt_after_retry: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&receipt_path).expect("connector receipt survives retry"),
    )
    .expect("connector receipt JSON after retry");
    assert_eq!(
        receipt_after_retry["after"]["created"], true,
        "retry must preserve first-success creation truth"
    );
}

#[test]
fn authenticated_app_hold_endpoint_stages_named_apple_promotion() {
    let fixture = Fixture::new();
    let port = available_port();
    let child = fixture
        .heiwa()
        .env("HEIWA_MACHINE_AUTH_TOKEN", "apple-connector-test-token")
        .args(["app", "start", "--port", &port.to_string(), "--no-open"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start temporary runtime");
    let _child = ChildGuard(child);
    wait_for_runtime(port);

    let response = post_json(
        port,
        "/api/v1/calendar/holds",
        &serde_json::json!({
            "title": "App-staged focus block",
            "date": "2026-06-19",
            "start": "16:00",
            "end": "16:30",
            "kind": "focus",
            "promotion": {
                "connector": "apple_calendar",
                "calendar": "Calendar"
            }
        }),
    );
    assert!(
        response.starts_with("HTTP/1.1 201 Created\r\n"),
        "unexpected response: {response}"
    );
    let body = response.split("\r\n\r\n").nth(1).expect("response body");
    let payload: serde_json::Value = serde_json::from_str(body).expect("response JSON");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["data"]["hold"]["promotion"]["calendar"], "Calendar");
    assert_eq!(payload["data"]["approval_request"]["risk_tier"], "T2");
    assert_eq!(
        payload["data"]["approval_request"]["work_id"],
        payload["data"]["hold"]["work_id"]
    );
    assert_eq!(
        fs::read_to_string(&fixture.log).expect("bridge log after app staging"),
        "list\n",
        "the app may validate the target but must not create before approval"
    );
}
