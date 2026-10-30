//! Deterministic user/agent tools share the shipping CLI contract.
use serde_json::{json, Value};
use std::process::{Command, Output};
fn run(root: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", root)
        .env("HEIWA_HOME", root.join("runtime"))
        .env("PATH", "/usr/bin:/bin")
        .env("HEIWA_DISABLE_KEYCHAIN", "1")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}
fn data(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], "heiwa.cli/v1");
    assert_eq!(value["ok"], true);
    value["data"].clone()
}
#[test]
fn codex_verification_rejects_unknown_flags_before_starting_inference() {
    let root = tempfile::tempdir().unwrap();
    let result = run(
        root.path(),
        &["auth", "verify-codex", "--unknown", "--json"],
    );
    assert!(!result.status.success());
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["ok"], false);
    assert!(value.to_string().contains("heiwa auth verify-codex"));
}

#[test]
fn catalog_and_one_tool_schema_are_discoverable_without_inference_or_native_access() {
    let root = tempfile::tempdir().unwrap();
    let catalog = data(run(root.path(), &["tools", "list", "--json"]));
    assert!(catalog["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "notes.create"));
    let single = data(run(
        root.path(),
        &["tools", "show", "reminders.complete", "--json"],
    ));
    assert_eq!(
        single["tool"]["input_schema"]["properties"]["completed"]["type"],
        "boolean"
    );
}
#[test]
fn a_saved_payload_runs_and_is_verified_without_overwriting_existing_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let payload = root.path().join("payload.json");
    std::fs::write(
        &payload,
        serde_json::to_vec(&json!({"filename":"proof.txt","content":"tool payload fixture"}))
            .unwrap(),
    )
    .unwrap();
    let args = [
        "tools",
        "run",
        "artifact.write",
        "--args-file",
        payload.to_str().unwrap(),
        "--workspace",
        workspace.to_str().unwrap(),
        "--json",
    ];
    let result = data(run(root.path(), &args));
    assert_eq!(result["receipt"]["status"], "success");
    assert_eq!(result["receipt"]["result"]["verified"], true);
    assert_eq!(
        std::fs::read_to_string(workspace.join("proof.txt")).unwrap(),
        "tool payload fixture"
    );
    let repeated = run(root.path(), &args);
    assert!(!repeated.status.success());
    let failure: Value = serde_json::from_slice(&repeated.stdout).unwrap();
    assert_eq!(failure["ok"], false);
    assert_eq!(
        std::fs::read_to_string(workspace.join("proof.txt")).unwrap(),
        "tool payload fixture"
    );
}
#[cfg(target_os = "macos")]
#[test]
fn invalid_native_effects_are_rejected_before_permissions_or_ui_are_requested() {
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let helper = root.path().join("heiwa-apple-resources");
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../heiwa_app/desktop/native/AppleResources.swift");
    let compile = Command::new("xcrun")
        .args(["swiftc", "-parse-as-library"])
        .arg(source)
        .arg("-o")
        .arg(&helper)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    for request in [
        json!({"operation":"reminders_create","list_id":"fixture","title":""}),
        json!({"operation":"reminders_create","list_id":"fixture","title":"valid","due":false}),
        json!({"operation":"reminders_complete","list_id":"fixture","id":"one","expected_title":"fixture","completed":1}),
        json!({"operation":"files_select","schema_version":2}),
    ] {
        let mut child = Command::new(&helper)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&request).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["error"], "invalid_request");
    }
}
