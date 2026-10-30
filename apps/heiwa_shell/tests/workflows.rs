//! Drive packaged workflows through the real CLI with an isolated profile.
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn run(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", home)
        .env("HEIWA_HOME", home.join(".heiwa"))
        .env("HEIWA_DISABLE_KEYCHAIN", "1")
        .env("TZ", "UTC")
        .env("PATH", "/usr/bin:/bin")
        // Any accidental native read would fail, rather than reaching the user's helper.
        .env(
            "HEIWA_APPLE_RESOURCES_HELPER",
            home.join("no-native-helper"),
        )
        .current_dir(home)
        .args(args)
        .output()
        .expect("run CLI")
}

fn data(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("one JSON envelope");
    assert_eq!(value["schema"], "heiwa.cli/v1");
    assert_eq!(value["ok"], true);
    value["data"].clone()
}

#[test]
fn workflows_are_discoverable_and_all_run_without_bootstrapping_a_profile() {
    let home = tempfile::tempdir().unwrap();
    let help = data(run(home.path(), &["help", "--json"]));
    assert!(help["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["name"] == "workflows"));
    let catalog = data(run(home.path(), &["workflows", "list", "--json"]));
    for row in catalog["workflows"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let shown = data(run(home.path(), &["workflows", "show", id, "--json"]));
        assert_eq!(shown, *row);
        let result = data(run(home.path(), &["workflows", "run", id, "--json"]));
        assert_eq!(result["id"], id);
        assert_eq!(result["workflow_schema"], "heiwa.workflows/v1");
        assert!(result["policy"]
            .as_str()
            .unwrap()
            .contains("no model calls"));
    }
    assert!(
        !home.path().join(".heiwa").exists(),
        "read workflows must not initialize or change a profile"
    );
}

#[test]
fn daily_brief_limits_rows_and_excludes_other_days_and_mail_bodies() {
    let home = tempfile::tempdir().unwrap();
    let state = home.path().join(".heiwa/state");
    fs::create_dir_all(state.join("calendar")).unwrap();
    fs::create_dir_all(state.join("mail")).unwrap();
    let today = chrono::Utc::now().date_naive().to_string();
    let mut events = Vec::new();
    for n in 0..3 {
        events.push(
            json!({"id":format!("event-{n}"), "date":today,
            "start":format!("{today}T09:00:00Z"), "end":format!("{today}T10:00:00Z"),
            "title":format!("today {n}"), "source":"fixture", "body":"CALENDAR_BODY_SENTINEL"})
            .to_string(),
        );
    }
    events.push(
        json!({"id":"old", "date":"2000-01-01", "start":"2000-01-01T09:00:00Z",
        "end":"2000-01-01T10:00:00Z", "title":"OLD_EVENT_SENTINEL"})
        .to_string(),
    );
    fs::write(state.join("calendar/events.jsonl"), events.join("\n")).unwrap();
    let mail = (0..3)
        .map(|n| {
            json!({"id":format!("mail-{n}"), "subject":"urgent action required",
        "sender":"fixture@example.invalid", "unread":true, "date":today,
        "body":"MAIL_BODY_SENTINEL"})
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(state.join("mail/headers.jsonl"), &mail).unwrap();
    let result = data(run(
        home.path(),
        &["workflows", "run", "daily-brief", "--limit", "1", "--json"],
    ));
    let brief = &result["result"];
    for rows in [&brief["calendar"]["events"], &brief["mail"]["priority"]] {
        assert_eq!(rows["items"].as_array().unwrap().len(), 1);
        assert_eq!(rows["available"], 3);
        assert_eq!(rows["truncated"], true);
    }
    let text = result.to_string();
    for hidden in [
        "OLD_EVENT_SENTINEL",
        "CALENDAR_BODY_SENTINEL",
        "MAIL_BODY_SENTINEL",
    ] {
        assert!(!text.contains(hidden), "unexpected content: {hidden}");
    }
    assert_eq!(
        fs::read_to_string(state.join("mail/headers.jsonl")).unwrap(),
        mail
    );
}

#[test]
fn invalid_flags_and_limits_are_usage_errors() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        vec!["run", "daily-brief", "--limit", "0"],
        vec!["run", "daily-brief", "--limit", "51"],
        vec!["run", "daily-brief", "--limit"],
        vec!["run", "daily-brief", "--limit", "1", "--limit", "2"],
        vec!["run", "native-audit", "--limit", "2"],
        vec!["run", "daily-brief", "--script", "anything"],
    ] {
        let mut command = vec!["workflows"];
        command.extend(args);
        command.push("--json");
        let output = run(home.path(), &command);
        assert_eq!(output.status.code(), Some(2), "{command:?}");
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["code"], "usage");
    }
}

#[test]
fn unknown_workflow_is_not_found_and_never_evaluated_as_a_command() {
    let home = tempfile::tempdir().unwrap();
    let output = run(
        home.path(),
        &["workflows", "run", "$(touch SENTINEL)", "--json"],
    );
    assert_eq!(output.status.code(), Some(10));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "not_found");
    assert!(!home.path().join("SENTINEL").exists());
}
