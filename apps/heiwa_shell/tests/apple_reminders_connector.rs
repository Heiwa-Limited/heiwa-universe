//! Hermetic Apple Reminders CLI acceptance through the native-helper stdin seam.
//!
//! The helper fixture records every request and only answers list/scan reads;
//! tests never touch EventKit, TCC, a provider, or a user's Apple data.
#![cfg(unix)]

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

mod cli_v1;

const LIST_ID: &str = "list-selected";

struct Fixture {
    _root: tempfile::TempDir,
    home: PathBuf,
    runtime_root: PathBuf,
    state: PathBuf,
    helper: PathBuf,
    request_log: PathBuf,
    list_response: PathBuf,
    scan_response: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temporary fixture root");
        let home = root.path().join("home");
        let runtime_root = home.join(".heiwa");
        let state = runtime_root.join("state");
        fs::create_dir_all(runtime_root.join("connectors")).expect("create isolated profile");

        fs::write(
            runtime_root.join("local-identity.json"),
            json!({
                "version": 1,
                "installation_id": "install-reminders-test",
                "display_name": "Fixture",
                "created_at": "2026-10-01T00:00:00Z"
            })
            .to_string(),
        )
        .expect("write isolated identity");
        fs::write(
            runtime_root.join("machine.json"),
            json!({
                "schema_version": "heiwa_machine_v1",
                "device_id": "device-reminders-test",
                "hostname": "fixture",
                "os": "macos",
                "arch": "arm64",
                "installed_at": "2026-10-01T00:00:00Z",
                "runtimes": {
                    "rust_version": null,
                    "node_version": null,
                    "python_version": null,
                    "claude_installed": false,
                    "codex_installed": false,
                    "gemini_installed": false,
                    "antigravity_installed": false,
                    "ollama_installed": false
                }
            })
            .to_string(),
        )
        .expect("write isolated machine identity");

        let request_log = root.path().join("helper-requests.jsonl");
        let list_response = root.path().join("reminders-list.json");
        let scan_response = root.path().join("reminders-scan.json");
        fs::write(
            &list_response,
            json!({
                "schema_version": 1,
                "truncated": false,
                "lists": [
                    {"id": LIST_ID, "name": "Selected", "source": "On My Mac", "writable": true},
                    {"id": "list-other", "name": "Other", "source": "iCloud", "writable": true},
                    {"id": "list-readonly", "name": "Read only", "source": "iCloud", "writable": false}
                ]
            })
            .to_string(),
        )
        .expect("write list response");
        fs::write(
            &scan_response,
            json!({
                "schema_version": 1,
                "list_ids": [LIST_ID],
                "reminders": [],
                "truncated": false
            })
            .to_string(),
        )
        .expect("write empty scan response");

        let helper = root.path().join("fixture-apple-resources");
        fs::write(
            &helper,
            r#"#!/usr/bin/env python3
import json, os, sys
assert len(sys.argv) == 1, "helper requests must be sent on stdin, never argv"
raw = sys.stdin.buffer.read()
request = json.loads(raw)
with open(os.environ["HEIWA_REMINDERS_FIXTURE_LOG"], "ab") as log:
    log.write(raw + b"\n")
operation = request.get("operation")
if operation == "reminders_list":
    with open(os.environ["HEIWA_REMINDERS_FIXTURE_LIST"], "rb") as source:
        sys.stdout.buffer.write(source.read())
elif operation == "reminders_scan":
    if os.environ.get("HEIWA_REMINDERS_FIXTURE_FAIL_SCAN") == "1":
        sys.stderr.write("fixture scan unavailable")
        sys.exit(92)
    with open(os.environ["HEIWA_REMINDERS_FIXTURE_SCAN"], "rb") as source:
        sys.stdout.buffer.write(source.read())
else:
    sys.stderr.write("fixture denies non-read helper operation")
    sys.exit(91)
"#,
        )
        .expect("write read-only helper fixture");
        let mut permissions = fs::metadata(&helper)
            .expect("helper metadata")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&helper, permissions).expect("make helper executable");

        Self {
            _root: root,
            home,
            runtime_root,
            state,
            helper,
            request_log,
            list_response,
            scan_response,
        }
    }

    fn heiwa(&self) -> Command {
        // Run these same hermetic cases against a hashed packaged development
        // runtime on device, without replacing Cargo's or the installed binary.
        let binary = std::env::var_os("HEIWA_REMINDERS_TEST_BINARY")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_heiwa").into());
        let mut command = Command::new(binary);
        command
            .env("HOME", &self.home)
            .env("HEIWA_HOME", &self.runtime_root)
            .env("HEIWA_STATE_DIR", &self.state)
            .env("HEIWA_APPLE_RESOURCES_HELPER", &self.helper)
            .env("HEIWA_REMINDERS_FIXTURE_LOG", &self.request_log)
            .env("HEIWA_REMINDERS_FIXTURE_LIST", &self.list_response)
            .env("HEIWA_REMINDERS_FIXTURE_SCAN", &self.scan_response)
            .env_remove("HEIWA_APPLE_CALENDAR_OSASCRIPT")
            .env_remove("HEIWA_HOME_OVERRIDE")
            .env_remove("HEIWA_EVIDENCE_DIR");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.heiwa().args(args).output().expect("run heiwa")
    }

    fn connect_and_select_list(&self) {
        let connected = self.run(&["connect", "apple-reminders", "--authorize", "--json"]);
        assert!(
            connected.status.success(),
            "connect failed: {}",
            String::from_utf8_lossy(&connected.stdout)
        );
        assert!(
            !self
                .runtime_root
                .join("state/connectors/apple_calendar.json")
                .exists(),
            "Reminders enrollment must not enroll Calendar"
        );
        let selected = self.run(&["reminders", "select", LIST_ID, "--json"]);
        assert!(
            selected.status.success(),
            "select failed: {}",
            String::from_utf8_lossy(&selected.stdout)
        );
    }

    fn select_calendar_and_imported_event(&self) {
        self.seed_calendar_enrollment();
        let calendar = self.state.join("calendar");
        fs::create_dir_all(&calendar).expect("create calendar state");
        fs::write(
            calendar.join("apple_selection.json"),
            json!({ "schema_version": 1, "calendar_ids": ["calendar-fixture"] }).to_string(),
        )
        .expect("write selected calendar");
        fs::write(
            calendar.join("events.jsonl"),
            format!(
                "{}\n",
                json!({
                    "source": "apple_calendar",
                    "calendar_id": "calendar-fixture",
                    "external_id": "event-fixture",
                    "occurrence": "2026-11-03T10:00:00-08:00",
                    "status": "confirmed",
                    "all_day": false,
                    "title": "Fixture appointment",
                    "start": "2026-11-03T10:00:00-08:00",
                    "end": "2026-11-03T10:30:00-08:00",
                    "synced_at": "2026-10-02T00:00:00Z"
                })
            ),
        )
        .expect("write imported Calendar event");
    }

    fn write_imported_events(&self, events: &[Value], selected_calendars: &[&str]) {
        self.seed_calendar_enrollment();
        let calendar = self.state.join("calendar");
        fs::create_dir_all(&calendar).expect("create calendar state");
        fs::write(
            calendar.join("apple_selection.json"),
            json!({ "schema_version": 1, "calendar_ids": selected_calendars }).to_string(),
        )
        .expect("write selected calendars");
        let lines = events
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(
            calendar.join("events.jsonl"),
            if lines.is_empty() {
                lines
            } else {
                format!("{lines}\n")
            },
        )
        .expect("write imported Calendar events");
    }

    fn seed_calendar_enrollment(&self) {
        let path = self
            .runtime_root
            .join("state/connectors/apple_calendar.json");
        fs::create_dir_all(path.parent().unwrap()).expect("create connector state");
        fs::write(
            path,
            json!({
                "schema_version": "heiwa_connector_enrollment_v1",
                "connector": "apple_calendar",
                "installation_id": "install-reminders-test",
                "device_id": "device-reminders-test",
                "connected_at": "2026-10-01T00:00:00Z",
                "scopes": ["calendar.read"]
            })
            .to_string(),
        )
        .expect("seed independent Calendar enrollment");
    }

    fn seed_reminders_enrollment(
        &self,
        schema_version: &str,
        installation_id: &str,
        device_id: &str,
    ) {
        let path = self
            .runtime_root
            .join("state/connectors/apple_reminders.json");
        fs::create_dir_all(path.parent().unwrap()).expect("create connector state");
        fs::write(
            path,
            json!({
                "schema_version": schema_version,
                "connector": "apple_reminders",
                "installation_id": installation_id,
                "device_id": device_id,
                "connected_at": "2026-10-01T00:00:00Z",
                "scopes": ["reminders.read"]
            })
            .to_string(),
        )
        .expect("seed Reminders enrollment");
    }

    fn set_list_response(&self, response: Value) {
        fs::write(&self.list_response, response.to_string()).expect("write list response fixture");
    }

    fn clear_requests(&self) {
        let _ = fs::remove_file(&self.request_log);
    }

    fn helper_requests(&self) -> Vec<Value> {
        fs::read_to_string(&self.request_log)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).expect("request log JSON"))
            .collect()
    }

    fn set_scan(&self, list_ids: &[&str], reminders: Value, truncated: bool) {
        fs::write(
            &self.scan_response,
            json!({
                "schema_version": 1,
                "list_ids": list_ids,
                "reminders": reminders,
                "truncated": truncated
            })
            .to_string(),
        )
        .expect("write scan fixture");
    }

    fn set_scan_raw(&self, response: &[u8]) {
        fs::write(&self.scan_response, response).expect("write raw scan fixture");
    }

    fn propose(
        &self,
        list_id: &str,
        calendar_id: &str,
        external_id: &str,
        occurrence: &str,
    ) -> Output {
        self.run(&[
            "reminders",
            "propose",
            "--list",
            list_id,
            "--event",
            calendar_id,
            external_id,
            "--occurrence",
            occurrence,
            "--json",
        ])
    }

    fn proposed(
        &self,
        list_id: &str,
        calendar_id: &str,
        external_id: &str,
        occurrence: &str,
    ) -> Value {
        let output = self.propose(list_id, calendar_id, external_id, occurrence);
        assert!(
            output.status.success(),
            "propose failed: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        cli_v1::data(&output.stdout)
    }

    fn fail_scan(&self) -> Output {
        self.heiwa()
            .env("HEIWA_REMINDERS_FIXTURE_FAIL_SCAN", "1")
            .args([
                "reminders",
                "propose",
                "--list",
                LIST_ID,
                "--event",
                "calendar-fixture",
                "event-fixture",
                "--occurrence",
                "2026-11-03T10:00:00-08:00",
                "--json",
            ])
            .output()
            .expect("run propose with unavailable scan")
    }
}

fn event(calendar_id: &str, external_id: &str, occurrence: &str, title: &str) -> Value {
    json!({
        "source": "apple_calendar",
        "calendar_id": calendar_id,
        "external_id": external_id,
        "occurrence": occurrence,
        "status": "confirmed",
        "all_day": false,
        "title": title,
        "start": occurrence,
        "end": "2026-11-03T10:30:00-08:00",
        "synced_at": "2026-10-02T00:00:00Z"
    })
}

fn assert_cli_error(output: &Output) -> Value {
    assert!(
        !output.status.success(),
        "expected refusal, got success: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    cli_v1::error(&output.stdout)
}

#[test]
fn reminders_list_requires_its_own_enrollment_and_never_calls_helper_without_it() {
    let fixture = Fixture::new();
    fixture.seed_calendar_enrollment();

    let output = fixture.run(&["reminders", "lists", "--json"]);
    let error = assert_cli_error(&output);

    assert!(error["message"]
        .as_str()
        .unwrap_or_default()
        .contains("Reminders"));
    assert!(
        fixture.helper_requests().is_empty(),
        "helper ran before Reminders enrollment"
    );
    assert!(!fixture
        .runtime_root
        .join("state/connectors/apple_reminders.json")
        .exists());
}

#[test]
fn reminders_enrollment_is_bound_to_this_installation_and_device() {
    for (installation_id, device_id) in [
        ("other-install", "device-reminders-test"),
        ("install-reminders-test", "other-device"),
    ] {
        let fixture = Fixture::new();
        fixture.seed_reminders_enrollment(
            "heiwa_connector_enrollment_v1",
            installation_id,
            device_id,
        );

        let output = fixture.run(&["reminders", "lists", "--json"]);
        let error = assert_cli_error(&output);

        assert!(error["message"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase()
            .contains("install"));
        assert!(fixture.helper_requests().is_empty());
    }
}

#[test]
fn reconnect_never_overwrites_a_newer_reminders_enrollment_schema() {
    let fixture = Fixture::new();
    let path = fixture
        .runtime_root
        .join("state/connectors/apple_reminders.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        r#"{"schema_version":"heiwa_connector_enrollment_v9","connector":"apple_reminders","installation_id":"install-reminders-test","device_id":"device-reminders-test","connected_at":"2026-10-01T00:00:00Z","scopes":["reminders.read"],"future":true}"#,
    )
    .unwrap();
    let before = fs::read(&path).unwrap();

    let output = fixture.run(&["connect", "apple-reminders", "--authorize", "--json"]);
    let error = assert_cli_error(&output);

    assert!(
        error["message"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase()
            .contains("unsupported"),
        "unexpected reconnect error: {error}"
    );
    assert_eq!(fs::read(path).unwrap(), before, "future enrollment changed");
    assert!(
        fixture.helper_requests().is_empty(),
        "must validate before prompting helper"
    );
}

#[test]
fn reminders_permission_denial_is_actionable_and_does_not_enroll() {
    let fixture = Fixture::new();
    fixture.set_list_response(json!({
        "schema_version": 1,
        "error": "reminders_access_required"
    }));

    let output = fixture.run(&["connect", "apple-reminders", "--authorize", "--json"]);
    let error = assert_cli_error(&output);

    assert!(error["hint"]
        .as_str()
        .unwrap_or_default()
        .contains("Reminders"));
    assert!(
        !fixture
            .runtime_root
            .join("state/connectors/apple_reminders.json")
            .exists(),
        "denied TCC request must not enroll"
    );
    assert_eq!(fixture.helper_requests().len(), 1);
    assert_eq!(fixture.helper_requests()[0]["operation"], "reminders_list");
}

#[test]
fn empty_unknown_and_duplicate_selections_never_scan_a_wildcard() {
    let fixture = Fixture::new();
    fixture.connect_and_select_list();

    let unknown = fixture.run(&["reminders", "select", "not-a-list", "--json"]);
    assert_cli_error(&unknown);
    let duplicate = fixture.run(&["reminders", "select", LIST_ID, LIST_ID, "--json"]);
    assert_cli_error(&duplicate);

    let selection = fixture.state.join("reminders/apple_selection.json");
    let selected_before_empty = fs::read(&selection).unwrap();
    let empty = fixture.run(&["reminders", "select", "--json"]);
    assert_cli_error(&empty);
    assert_eq!(fs::read(selection).unwrap(), selected_before_empty);

    fs::write(
        fixture.state.join("reminders/apple_selection.json"),
        json!({"schema_version":1,"list_ids":[]}).to_string(),
    )
    .unwrap();
    fixture.clear_requests();
    let read = fixture.run(&["reminders", "read", "--json"]);
    assert_cli_error(&read);
    assert!(
        fixture.helper_requests().is_empty(),
        "empty selection reached EventKit helper"
    );
}

#[test]
fn calendar_enrollment_never_substitutes_for_reminders_admission() {
    let fixture = Fixture::new();
    fixture.seed_calendar_enrollment();

    let output = fixture.run(&["reminders", "lists", "--json"]);
    assert_cli_error(&output);
    assert!(fixture.helper_requests().is_empty());
    assert!(!fixture
        .runtime_root
        .join("state/connectors/apple_reminders.json")
        .exists());
}

#[test]
fn selected_known_writable_list_with_complete_empty_scan_proposes_create_without_writes() {
    let fixture = Fixture::new();
    fixture.connect_and_select_list();
    fixture.select_calendar_and_imported_event();
    fixture.set_scan(&[LIST_ID], json!([]), false);

    let output = fixture.run(&[
        "reminders",
        "propose",
        "--list",
        LIST_ID,
        "--event",
        "calendar-fixture",
        "event-fixture",
        "--occurrence",
        "2026-11-03T10:00:00-08:00",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "propose failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let data = cli_v1::data(&output.stdout);
    assert_eq!(data["outcome"], "create");
    assert_eq!(data["proposed"]["title"], "Fixture appointment");
    assert_eq!(
        data["proposed"]["due"],
        json!({"instant":"2026-11-03T18:00:00Z"})
    );
    assert!(data["proposed"]["marker"].as_str().is_some_and(|marker| {
        marker.starts_with("heiwa://effect/rem-evt-")
            && marker.len() == "heiwa://effect/rem-evt-".len() + 64
    }));
    assert_eq!(
        data["proposed"]["source_event"]["calendar_id"],
        "calendar-fixture"
    );
    assert_eq!(
        data["proposed"]["source_event"]["external_id"],
        "event-fixture"
    );

    let requests = fixture.helper_requests();
    assert!(!requests.is_empty());
    assert!(
        requests.iter().all(|request| matches!(
            request["operation"].as_str(),
            Some("reminders_list" | "reminders_scan")
        )),
        "fixture saw non-read helper operation: {requests:?}"
    );
    let scan = requests
        .iter()
        .find(|request| request["operation"] == "reminders_scan")
        .unwrap();
    assert_eq!(scan["list_ids"], json!([LIST_ID]));
}

#[test]
fn incomplete_scan_cannot_claim_create_or_in_sync() {
    let fixture = Fixture::new();
    fixture.connect_and_select_list();
    fixture.select_calendar_and_imported_event();
    fixture.set_scan(
        &[LIST_ID],
        json!([{
            "id": "existing-reminder",
            "list_id": LIST_ID,
            "title": "Fixture appointment",
            "due": null,
            "completed": false,
            "marker": null
        }]),
        true,
    );

    let output = fixture.run(&[
        "reminders",
        "propose",
        "--list",
        LIST_ID,
        "--event",
        "calendar-fixture",
        "event-fixture",
        "--occurrence",
        "2026-11-03T10:00:00-08:00",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "partial evidence should be represented, not promoted to a hard failure: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let data = cli_v1::data(&output.stdout);
    assert_eq!(data["outcome"], "undetermined");
    assert_ne!(data["outcome"], "create");
    assert_ne!(data["outcome"], "in_sync");
}

#[test]
fn incomplete_inventory_or_malformed_scan_cannot_claim_create_or_in_sync() {
    let fixture = Fixture::new();
    fixture.connect_and_select_list();
    fixture.select_calendar_and_imported_event();
    fixture.set_list_response(json!({
        "schema_version": 1,
        "lists": [
            {"id": LIST_ID, "name": "Selected", "source": "On My Mac", "writable": true},
            {"id": "list-other", "name": "Other", "source": "iCloud", "writable": true},
            {"id": "list-readonly", "name": "Read only", "source": "iCloud", "writable": false}
        ],
        "truncated": true
    }));
    fixture.set_scan(&[LIST_ID], json!([]), false);
    let inventory_truncated = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "event-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    assert_eq!(inventory_truncated["outcome"], "undetermined");
    assert_eq!(inventory_truncated["scan_complete"], false);

    fixture.set_list_response(json!({
        "schema_version": 1,
        "lists": [
            {"id": LIST_ID, "name": "Selected", "source": "On My Mac", "writable": true},
            {"id": "list-other", "name": "Other", "source": "iCloud", "writable": true},
            {"id": "list-readonly", "name": "Read only", "source": "iCloud", "writable": false}
        ],
        "truncated": false
    }));
    fixture.set_scan_raw(b"{malformed json");
    let malformed = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "event-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    assert_eq!(malformed["outcome"], "undetermined");
    assert_eq!(malformed["scan_complete"], false);
}

#[test]
fn event_identity_is_stable_and_recurring_occurrences_remain_distinct() {
    let fixture = Fixture::new();
    fixture.connect_and_select_list();
    fixture.write_imported_events(
        &[
            event(
                "calendar-fixture",
                "series-fixture",
                "2026-11-03T10:00:00-08:00",
                "Recurring appointment",
            ),
            event(
                "calendar-fixture",
                "series-fixture",
                "2026-11-10T10:00:00-08:00",
                "Recurring appointment",
            ),
        ],
        &["calendar-fixture"],
    );
    fixture.set_scan(&[LIST_ID], json!([]), false);

    let first = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "series-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    let first_again = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "series-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    let next_occurrence = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "series-fixture",
        "2026-11-10T10:00:00-08:00",
    );

    let first_marker = first["proposed"]["marker"]
        .as_str()
        .expect("first effect marker");
    let next_marker = next_occurrence["proposed"]["marker"]
        .as_str()
        .expect("next effect marker");
    assert_eq!(first_marker, first_again["proposed"]["marker"]);
    assert_ne!(first_marker, next_marker);
    for marker in [first_marker, next_marker] {
        let digest = marker.strip_prefix("heiwa://effect/rem-evt-").unwrap();
        assert_eq!(digest.len(), 64);
        assert!(digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    }
}

#[test]
fn matching_marker_is_in_sync_but_changed_or_completed_marker_is_conflict() {
    let fixture = Fixture::new();
    fixture.connect_and_select_list();
    fixture.select_calendar_and_imported_event();
    fixture.set_scan(&[LIST_ID], json!([]), false);
    let created = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "event-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    let marker = created["proposed"]["marker"].clone();

    fixture.set_scan(
        &[LIST_ID],
        json!([{
            "id": "existing",
            "list_id": LIST_ID,
            "title": "Fixture appointment",
            "due": {"instant":"2026-11-03T18:00:00Z"},
            "completed": false,
            "marker": marker
        }]),
        false,
    );
    let matching = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "event-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    assert_eq!(matching["outcome"], "in_sync");

    fixture.set_scan(
        &[LIST_ID],
        json!([{
            "id": "existing",
            "list_id": LIST_ID,
            "title": "Edited by user",
            "due": {"instant":"2026-11-03T18:00:00Z"},
            "completed": false,
            "marker": created["proposed"]["marker"]
        }]),
        false,
    );
    let changed = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "event-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    assert_eq!(changed["outcome"], "conflict");

    fixture.set_scan(
        &[LIST_ID],
        json!([{
            "id": "existing",
            "list_id": LIST_ID,
            "title": "Fixture appointment",
            "due": {"instant":"2026-11-03T18:00:00Z"},
            "completed": true,
            "marker": created["proposed"]["marker"]
        }]),
        false,
    );
    let completed = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "event-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    assert_eq!(
        completed["outcome"], "conflict",
        "completion must never be coerced to in_sync"
    );
}

#[test]
fn duplicate_markers_are_ambiguous_and_marker_in_another_selected_list_is_not_create() {
    let fixture = Fixture::new();
    fixture.connect_and_select_list();
    let selected_two = fixture.run(&["reminders", "select", LIST_ID, "list-other", "--json"]);
    assert!(selected_two.status.success());
    fixture.select_calendar_and_imported_event();
    fixture.set_scan(&[LIST_ID, "list-other"], json!([]), false);
    let created = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "event-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    let marker = created["proposed"]["marker"].clone();

    fixture.set_scan(
        &[LIST_ID, "list-other"],
        json!([
            {"id":"one","list_id":LIST_ID,"title":"Fixture appointment","due":null,"completed":false,"marker":marker},
            {"id":"two","list_id":LIST_ID,"title":"Fixture appointment","due":null,"completed":false,"marker":marker}
        ]),
        false,
    );
    let duplicate = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "event-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    assert_eq!(duplicate["outcome"], "ambiguous");

    fixture.set_scan(
        &[LIST_ID, "list-other"],
        json!([{
            "id":"other-list",
            "list_id":"list-other",
            "title":"Fixture appointment",
            "due":null,
            "completed":false,
            "marker":marker
        }]),
        false,
    );
    let elsewhere = fixture.proposed(
        LIST_ID,
        "calendar-fixture",
        "event-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    assert_eq!(elsewhere["outcome"], "exists_in_other_list");
}

#[test]
fn read_only_target_and_unknown_or_unselected_inputs_are_never_proposed() {
    let fixture = Fixture::new();
    fixture.connect_and_select_list();
    fixture.select_calendar_and_imported_event();
    fixture.set_scan(&["list-readonly"], json!([]), false);

    let readonly_selected = fixture.run(&["reminders", "select", "list-readonly", "--json"]);
    assert!(readonly_selected.status.success());
    let readonly = fixture.propose(
        "list-readonly",
        "calendar-fixture",
        "event-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    assert!(readonly.status.success());
    assert_eq!(cli_v1::data(&readonly.stdout)["outcome"], "list_read_only");

    fixture.set_scan(&["list-readonly"], json!([]), false);
    let unknown_target = fixture.propose(
        "not-a-list",
        "calendar-fixture",
        "event-fixture",
        "2026-11-03T10:00:00-08:00",
    );
    assert_cli_error(&unknown_target);

    fixture.write_imported_events(
        &[event(
            "calendar-not-selected",
            "outside-event",
            "2026-11-03T10:00:00-08:00",
            "Outside event",
        )],
        &["calendar-fixture"],
    );
    let unselected_calendar = fixture.propose(
        "list-readonly",
        "calendar-not-selected",
        "outside-event",
        "2026-11-03T10:00:00-08:00",
    );
    assert_cli_error(&unselected_calendar);
}

#[test]
fn unavailable_scan_is_undetermined_and_read_projection_drops_notes_and_raw_urls() {
    let fixture = Fixture::new();
    fixture.connect_and_select_list();
    fixture.select_calendar_and_imported_event();
    let unavailable = fixture.fail_scan();
    assert!(
        unavailable.status.success(),
        "unavailable scan should yield conservative result: {}",
        String::from_utf8_lossy(&unavailable.stdout)
    );
    let data = cli_v1::data(&unavailable.stdout);
    assert_eq!(data["outcome"], "undetermined");

    fixture.set_scan(
        &[LIST_ID],
        json!([{
            "id":"fixture-reminder",
            "list_id":LIST_ID,
            "title":"Fixture reminder",
            "due":null,
            "completed":false,
            "marker":null,
            "notes":"PRIVATE_NOTES_SENTINEL",
            "url":"https://private.example.invalid/raw"
        }]),
        false,
    );
    let read = fixture.run(&["reminders", "read", "--json"]);
    assert!(
        read.status.success(),
        "read failed: {}",
        String::from_utf8_lossy(&read.stdout)
    );
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&read.stdout),
        String::from_utf8_lossy(&read.stderr)
    );
    assert!(!output.contains("PRIVATE_NOTES_SENTINEL"));
    assert!(!output.contains("private.example.invalid"));
}
