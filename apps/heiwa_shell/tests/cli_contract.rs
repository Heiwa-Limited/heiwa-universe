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
        assert!(names.contains(&expected), "{expected} missing from {names:?}");
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
