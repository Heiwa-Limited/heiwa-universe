use std::process::Command;
use std::process::Stdio;
use std::io::Write;

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
