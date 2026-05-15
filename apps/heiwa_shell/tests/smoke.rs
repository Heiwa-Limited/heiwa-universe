use std::io::Write;
use std::process::Command;
use std::process::Stdio;

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
        .args(&[
            "run",
            "-p",
            "heiwa-shell",
            "--bin",
            "heiwa",
            "--",
            "providers",
        ])
        .output()
        .expect("failed to execute process");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Registry auto-discovers ollama; CLI discovery shows known providers
    assert!(
        stdout.contains("ollama"),
        "expected ollama in provider list: {stdout}"
    );
    assert!(
        stdout.contains("[loop]"),
        "expected explicit loop capability marker: {stdout}"
    );
    // CLI discovery should surface known providers not yet in the registry
    assert!(
        stdout.contains("claude")
            || stdout.contains("antigravity")
            || stdout.contains("CLI Discovery"),
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
fn test_shell_supports_cwd_and_directory_scope_commands() {
    let output = run_shell_script("/cwd\n/add-dir ~/*\n/dirs\nquit\n");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("cwd:"),
        "expected /cwd to report the working directory: {stdout}"
    );
    assert!(
        stdout.contains("allowed dirs:"),
        "expected /dirs to report allowed directories: {stdout}"
    );
    assert!(
        !stdout.contains("unknown command: /cwd"),
        "expected /cwd to be handled: {stdout}"
    );
    assert!(
        !stdout.contains("unknown command: /add-dir"),
        "expected /add-dir to be handled: {stdout}"
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
fn test_route_preview_greeting_does_not_execute_model() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "heiwa-shell",
            "--bin",
            "heiwa",
            "--",
            "route",
            "preview",
            "hi",
        ])
        .output()
        .expect("failed to execute route preview");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("mode: deterministic"),
        "expected deterministic route preview: {stdout}"
    );
    assert!(
        stdout.contains("Ready"),
        "expected deterministic response body: {stdout}"
    );
}

#[test]
fn test_life_status_json_reports_sources_and_stdb_mode() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "heiwa-shell",
            "--bin",
            "heiwa",
            "--",
            "life",
            "status",
            "--json",
        ])
        .output()
        .expect("failed to execute life status");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"command\":\"life status\""),
        "expected life status json command marker: {stdout}"
    );
    assert!(
        stdout.contains("\"stdb_mode\":"),
        "expected STDB mode in life status json: {stdout}"
    );
    assert!(
        stdout.contains("\"home\""),
        "expected home source group in life status json: {stdout}"
    );
}

#[test]
fn test_life_import_home_dry_run_jsonl_counts_stdb_rows() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "heiwa-shell",
            "--bin",
            "heiwa",
            "--",
            "life",
            "import",
            "home",
            "--dry-run",
            "--jsonl",
        ])
        .output()
        .expect("failed to execute life import dry run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"dry_run\":true"),
        "expected dry-run marker in import jsonl: {stdout}"
    );
    assert!(
        stdout.contains("\"table\":\"life_sources\""),
        "expected life_sources row count in import jsonl: {stdout}"
    );
    assert!(
        stdout.contains("\"table\":\"life_memory_events\""),
        "expected life_memory_events row count in import jsonl: {stdout}"
    );
}

#[test]
fn test_app_help_exposes_boot_command_boundary() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "heiwa-shell",
            "--bin",
            "heiwa",
            "--",
            "app",
            "--help",
        ])
        .output()
        .expect("failed to execute app help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("heiwa app"),
        "expected app help to expose boot command: {stdout}"
    );
    assert!(
        stdout.contains("runtime status"),
        "expected app help to include runtime status command: {stdout}"
    );
    assert!(
        stdout.contains("--json"),
        "expected app help to include --json flag: {stdout}"
    );
}

#[test]
fn test_app_runtime_status_json_reports_local_probe() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "heiwa-shell",
            "--bin",
            "heiwa",
            "--",
            "app",
            "runtime",
            "status",
            "--json",
        ])
        .output()
        .expect("failed to execute app runtime status");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"command\":\"app runtime status\""),
        "expected app runtime status json command marker: {stdout}"
    );
    assert!(
        stdout.contains("\"policy\":\"local-only-no-side-effects\""),
        "expected local-only policy marker: {stdout}"
    );
    assert!(
        stdout.contains("\"workers\""),
        "expected workers summary in runtime status: {stdout}"
    );
    assert!(
        stdout.contains("\"approvals\""),
        "expected approvals summary in runtime status: {stdout}"
    );
    assert!(
        stdout.contains("\"mail\""),
        "expected mail summary in runtime status: {stdout}"
    );
    assert!(
        stdout.contains("\"keep_awake\""),
        "expected keep_awake probe in runtime status: {stdout}"
    );
}

#[test]
fn test_workers_heartbeat_dry_run_emits_local_envelope() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "heiwa-shell",
            "--bin",
            "heiwa",
            "--",
            "workers",
            "heartbeat",
            "--class",
            "shell_machine",
            "--id",
            "smoke-test-worker",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("failed to execute workers heartbeat dry-run");

    assert!(
        output.status.success(),
        "workers heartbeat dry-run should succeed"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"command\":\"workers heartbeat\""),
        "expected workers heartbeat json marker: {stdout}"
    );
    assert!(
        stdout.contains("\"dry_run\":true"),
        "expected dry_run true in workers heartbeat: {stdout}"
    );
    assert!(
        stdout.contains("\"worker_id\":\"smoke-test-worker\""),
        "expected smoke-test-worker id: {stdout}"
    );
    assert!(
        stdout.contains("\"class\":\"shell_machine\""),
        "expected shell_machine class: {stdout}"
    );
}

#[test]
fn test_workers_status_json_reports_registry_path() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "heiwa-shell",
            "--bin",
            "heiwa",
            "--",
            "workers",
            "status",
            "--json",
        ])
        .output()
        .expect("failed to execute workers status");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"command\":\"workers status\""),
        "expected workers status json marker: {stdout}"
    );
    assert!(
        stdout.contains("\"path\":"),
        "expected workers.json path in status: {stdout}"
    );
}

#[test]
fn test_approvals_list_json_reports_dispatch_paths() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "heiwa-shell",
            "--bin",
            "heiwa",
            "--",
            "approvals",
            "list",
            "--json",
        ])
        .output()
        .expect("failed to execute approvals list");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"command\":\"approvals list\""),
        "expected approvals list json marker: {stdout}"
    );
    let normalized_stdout = stdout.replace('\\', "/");
    assert!(
        normalized_stdout.contains("dispatch/requests"),
        "expected dispatch/requests directory in approvals list: {stdout}"
    );
    assert!(
        normalized_stdout.contains("dispatch/approvals/decisions"),
        "expected dispatch/approvals/decisions directory in approvals list: {stdout}"
    );
}

#[test]
fn test_mail_status_json_enforces_metadata_only_policy() {
    let output = Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "heiwa-shell",
            "--bin",
            "heiwa",
            "--",
            "mail",
            "status",
            "--json",
        ])
        .output()
        .expect("failed to execute mail status");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"command\":\"mail status\""),
        "expected mail status json marker: {stdout}"
    );
    assert!(
        stdout.contains("\"policy\":\"metadata-only-no-body\""),
        "expected metadata-only policy guarantee: {stdout}"
    );
    assert!(
        stdout.contains("\"fields\""),
        "expected fields whitelist (account/mailbox/sender/subject/date/unread): {stdout}"
    );
}
