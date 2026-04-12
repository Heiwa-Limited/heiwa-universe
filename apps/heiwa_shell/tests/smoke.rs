use std::process::Command;
use std::process::Stdio;
use std::io::Write;
use std::{fs, path::{Path, PathBuf}};

#[test]
fn test_heiwa_help() {
    let output = Command::new("cargo")
        .args(&["run", "-p", "heiwa-shell", "--bin", "heiwa", "--", "--help"])
        .output()
        .expect("failed to execute process");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BYOK terminal agent"));
}

#[test]
fn test_heiwa_providers_lists_wrapped_and_loop_capable_surfaces_honestly() {
    let output = Command::new("cargo")
        .args(&["run", "-p", "heiwa-shell", "--bin", "heiwa", "--", "providers"])
        .output()
        .expect("failed to execute process");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Registry auto-discovers ollama; CLI discovery shows known providers
    assert!(stdout.contains("ollama"), "expected ollama in provider list: {stdout}");
    assert!(stdout.contains("[loop]"), "expected explicit loop capability marker: {stdout}");
    // CLI discovery should surface known providers not yet in the registry
    assert!(
        stdout.contains("claude") || stdout.contains("antigravity") || stdout.contains("CLI Discovery"),
        "expected CLI discovery section or known providers: {stdout}"
    );
}

fn run_shell_script(script: &str) -> std::process::Output {
    let mut child = Command::new("cargo")
        .args(&["run", "-p", "heiwa-shell", "--bin", "heiwa", "--", "shell"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn heiwa shell");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(script.as_bytes())
        .expect("write shell script");

    child.wait_with_output().expect("wait for shell output")
}

fn create_temp_home() -> PathBuf {
    let tmp = std::env::temp_dir().join(format!("heiwa-shell-runtime-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&tmp).expect("create temp home");
    tmp
}

fn run_heiwa_in_home(home: &Path, args: &[&str]) -> std::process::Output {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("apps dir")
        .parent()
        .expect("repo root")
        .to_path_buf();

    let output = Command::new("cargo")
        .args(&["run", "-p", "heiwa-shell", "--bin", "heiwa", "--"])
        .args(args)
        .env("HOME", home)
        .env("HEIWA_ROOT", &repo_root)
        .output()
        .expect("failed to execute heiwa command");

    output
}

fn run_heiwa_with_temp_home(args: &[&str]) -> (PathBuf, std::process::Output) {
    let tmp = create_temp_home();
    let output = run_heiwa_in_home(&tmp, args);
    (tmp, output)
}

#[test]
fn test_shell_supports_model_and_provider_slash_commands() {
    let output = run_shell_script("/model\n/provider\nquit\n");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Unknown slash command: /model"),
        "expected /model to be handled: {stdout}"
    );
    assert!(
        !stdout.contains("Unknown slash command: /provider"),
        "expected /provider to be handled: {stdout}"
    );
}

#[test]
fn test_shell_supports_route_status_and_clear_slash_commands() {
    let output = run_shell_script("/route\n/status\n/clear\nquit\n");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Unknown slash command: /route"),
        "expected /route to be handled: {stdout}"
    );
    assert!(
        !stdout.contains("Unknown slash command: /status"),
        "expected /status to be handled: {stdout}"
    );
    assert!(
        !stdout.contains("Unknown slash command: /clear"),
        "expected /clear to be handled: {stdout}"
    );
}

#[test]
fn test_shell_greeting_is_handled_without_model_requirement() {
    let output = run_shell_script("hi\nquit\n");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Ready"),
        "expected deterministic greeting response: {stdout}"
    );
    assert!(
        !stdout.contains("No models available. Connect a provider first."),
        "greetings should not require a model: {stdout}"
    );
}

#[test]
fn test_login_writes_structured_runtime_state_files() {
    let (home, output) = run_heiwa_with_temp_home(&["login", "test-token"]);

    assert!(output.status.success());
    assert!(
        home.join(".heiwa/state/identity.json").exists(),
        "expected structured identity path"
    );
    assert!(
        home.join(".heiwa/state/connection.json").exists(),
        "expected structured connection path"
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn test_doctor_reports_runtime_root_and_concise_mode() {
    let home = create_temp_home();

    let install = run_heiwa_in_home(&home, &["install"]);
    assert!(install.status.success(), "install should succeed");

    let doctor = run_heiwa_in_home(&home, &["doctor"]);
    assert!(doctor.status.success(), "doctor should succeed");

    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(stdout.contains(".heiwa"), "doctor should print runtime root: {stdout}");
    assert!(stdout.contains("concise"), "doctor should print default mode: {stdout}");

    let _ = fs::remove_dir_all(home);
}

#[test]
fn test_repair_rebuilds_runtime_seed_and_projection_files() {
    let home = create_temp_home();

    let repair = run_heiwa_in_home(&home, &["repair"]);
    assert!(repair.status.success(), "repair should succeed");

    assert!(
        home.join(".heiwa/modes/concise/MODE.md").exists(),
        "repair should seed concise mode"
    );
    assert!(
        home.join(".heiwa/generated/codex/config.toml").exists(),
        "repair should generate codex projection"
    );

    let _ = fs::remove_dir_all(home);
}
