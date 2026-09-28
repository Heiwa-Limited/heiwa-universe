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
