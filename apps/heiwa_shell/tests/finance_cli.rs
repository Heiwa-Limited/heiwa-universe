//! `heiwa finance` and `heiwa connect snaptrade|alpha-vantage` against a
//! throwaway home. None of these tests reach the network or the keychain:
//! every path exercised here fails closed before either.

use std::io::Write as _;
use std::process::{Command, Output, Stdio};

fn heiwa(home: &std::path::Path, args: &[&str], stdin: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_heiwa"));
    command
        .env("HOME", home)
        .env_remove("HEIWA_HOME")
        .env_remove("HEIWA_STATE_DIR")
        .env_remove("HEIWA_EVIDENCE_DIR")
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("binary runs");
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().expect("binary finishes")
}

fn json(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json output")
}

#[test]
fn a_fresh_install_summary_is_read_only_empty_and_points_at_connect() {
    let home = tempfile::tempdir().unwrap();
    let summary = json(&heiwa(home.path(), &["finance", "summary", "--json"], None));
    assert_eq!(summary["schema_version"], "heiwa_finance_summary_v1");
    assert_eq!(summary["policy"], "read_only");
    assert_eq!(summary["connections"]["brokerage"], false);
    assert!(summary["portfolio"].is_null());
    assert!(summary["next_actions"]
        .to_string()
        .contains("heiwa connect snaptrade"));
}

#[test]
fn settings_persist_the_benchmark_and_tfsa_room_privately() {
    let home = tempfile::tempdir().unwrap();
    let saved = json(&heiwa(
        home.path(),
        &[
            "finance",
            "settings",
            "--benchmark",
            "xeqt.to",
            "--tfsa-room",
            "7000",
            "--year",
            "2026",
            "--json",
        ],
        None,
    ));
    assert_eq!(saved["benchmark"], "XEQT.TO");
    assert_eq!(saved["tfsa_room"]["year"], 2026);
    assert_eq!(saved["tfsa_room"]["room_at_start"], 7000.0);

    let summary = json(&heiwa(home.path(), &["finance", "summary", "--json"], None));
    assert_eq!(summary["settings"]["benchmark"], "XEQT.TO");

    let settings = home.path().join(".heiwa/state/finance/settings.json");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&settings).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    let cleared = json(&heiwa(
        home.path(),
        &["finance", "settings", "--clear-tfsa-room", "--json"],
        None,
    ));
    assert!(cleared["tfsa_room"].is_null());
}

#[test]
fn settings_refuse_values_that_cannot_be_a_benchmark_or_a_room() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        vec!["finance", "settings", "--benchmark", "../etc"],
        vec!["finance", "settings", "--tfsa-room", "-5"],
        vec!["finance", "settings", "--tfsa-room", "lots"],
    ] {
        let output = heiwa(home.path(), &args, None);
        assert!(!output.status.success(), "{args:?} must fail");
    }
}

#[test]
fn a_sync_with_nothing_connected_explains_itself_without_network() {
    let home = tempfile::tempdir().unwrap();
    let output = heiwa(home.path(), &["finance", "sync", "--json"], None);
    let report = json(&output);
    assert_eq!(report["outcome"], "error");
    assert!(report["issues"].to_string().contains("heiwa connect"));
}

#[test]
fn a_consumer_key_is_never_accepted_on_the_command_line() {
    let home = tempfile::tempdir().unwrap();
    let output = heiwa(
        home.path(),
        &[
            "connect",
            "snaptrade",
            "--client-id",
            "TEST-CLIENT",
            "--consumer-key",
            "test-consumer-key",
        ],
        None,
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("stdin"), "{stderr}");
    assert!(!stderr.contains("test-consumer-key"));
    assert!(!home
        .path()
        .join(".heiwa/state/connectors/snaptrade.json")
        .exists());
}

#[test]
fn malformed_credentials_fail_before_the_network_or_keychain() {
    let home = tempfile::tempdir().unwrap();
    let output = heiwa(
        home.path(),
        &["connect", "snaptrade"],
        Some("not a client id\nkey\n"),
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("client ID"));

    let output = heiwa(home.path(), &["connect", "alpha-vantage"], Some("\n"));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("API key"));
    assert!(!home
        .path()
        .join(".heiwa/state/connectors/alpha_vantage.json")
        .exists());
}

#[test]
fn connector_status_lists_the_finance_lanes() {
    let home = tempfile::tempdir().unwrap();
    let payload = json(&heiwa(home.path(), &["connect", "status", "--json"], None));
    let rows = payload["connectors"].as_array().unwrap();
    let row = |id: &str| {
        rows.iter()
            .find(|row| row["id"] == id)
            .unwrap_or_else(|| panic!("{id} row"))
    };
    assert_eq!(row("snaptrade")["kind"], "finance");
    assert_eq!(row("snaptrade")["status"], "needs_auth");
    assert_eq!(row("alpha_vantage")["status"], "needs_auth");
    assert_eq!(row("bank_of_canada")["status"], "connected");
}
