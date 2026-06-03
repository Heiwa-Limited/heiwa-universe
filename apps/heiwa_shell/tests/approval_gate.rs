use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use heiwa_protocol::{ExecutionScope, RiskClass, ToolCall, ToolLease};
use heiwa_shell::agentic::execute_tool_calls;
use tempfile::tempdir;

static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn leased_scope(root: PathBuf) -> ExecutionScope {
    let mut scope = ExecutionScope::local_default(root);
    for name in ["fs.read", "fs.list", "repo.grep"] {
        scope.tool_leases.push(ToolLease {
            name: name.to_string(),
            risk_class: RiskClass::HostSafeReadonly,
            allowed: true,
        });
    }
    scope
}

#[tokio::test]
async fn test_approval_gate_approve_flow() {
    let _lock = ENV_MUTEX.lock().unwrap();

    let temp = tempdir().unwrap();
    let root = temp.path().to_path_buf();

    // Set environment overrides
    std::env::set_var("HOME", &root);
    std::env::set_var("HEIWA_AUTO_APPROVE", "none");
    std::env::set_var("HEIWA_SURFACE", "discord"); // discord ensures it holds on medium/high/critical

    // Prepare a high-risk tool call
    let tool_calls = vec![ToolCall {
        id: "call-123".to_string(),
        name: "deploy".to_string(),
        arguments: serde_json::json!({"target": "production"}),
    }];

    // Spawn a native OS thread that executes the tool calls
    let execute_handle = std::thread::spawn({
        let scope = leased_scope(root.clone());
        let rt = tokio::runtime::Handle::current();
        move || {
            rt.block_on(async move { execute_tool_calls(scope, tool_calls, "test", "test").await })
        }
    });

    // Wait and find the request id
    let requests_dir = root
        .join(".heiwa")
        .join("state")
        .join("dispatch")
        .join("requests");
    let mut request_id = None;
    let start = std::time::Instant::now();

    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(entries) = fs::read_dir(&requests_dir) {
            let files: Vec<_> = entries.flatten().collect();
            if !files.is_empty() {
                let path = files[0].path();
                let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
                request_id = Some(stem);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let id = request_id.expect("Expected a staged request file to be created");

    println!("TEST: root path is {:?}", root);

    // Write the approved decision file
    let decisions_dir = root
        .join(".heiwa")
        .join("state")
        .join("dispatch")
        .join("approvals")
        .join("decisions");
    fs::create_dir_all(&decisions_dir).unwrap();
    let decision_path = decisions_dir.join(format!("{}.json", id));

    println!("TEST: decision_path is {:?}", decision_path);

    let decision_json = serde_json::json!({
        "id": id,
        "outcome": "approved",
        "decided_at_utc": chrono::Utc::now().to_rfc3339(),
        "operator": "test-decider"
    });
    fs::write(
        &decision_path,
        serde_json::to_string_pretty(&decision_json).unwrap(),
    )
    .unwrap();

    // Await execution receipt by joining the thread
    let (receipts, transcript) = execute_handle.join().unwrap().unwrap();

    assert_eq!(receipts.len(), 1);
    // Since "deploy" is not in the registry, it should proceed to call the registry, returning UnknownTool error
    assert_eq!(receipts[0].status.as_str(), "failure");
    assert!(receipts[0]
        .error
        .as_ref()
        .unwrap()
        .contains("unknown tool: deploy"));
    assert!(transcript[0].output.contains("unknown tool: deploy"));
}

#[tokio::test]
async fn test_approval_gate_deny_flow() {
    let _lock = ENV_MUTEX.lock().unwrap();

    let temp = tempdir().unwrap();
    let root = temp.path().to_path_buf();

    std::env::set_var("HOME", &root);
    std::env::set_var("HEIWA_AUTO_APPROVE", "none");
    std::env::set_var("HEIWA_SURFACE", "discord");

    let tool_calls = vec![ToolCall {
        id: "call-456".to_string(),
        name: "deploy".to_string(),
        arguments: serde_json::json!({"target": "production"}),
    }];

    let execute_handle = std::thread::spawn({
        let scope = leased_scope(root.clone());
        let rt = tokio::runtime::Handle::current();
        move || {
            rt.block_on(async move { execute_tool_calls(scope, tool_calls, "test", "test").await })
        }
    });

    let requests_dir = root
        .join(".heiwa")
        .join("state")
        .join("dispatch")
        .join("requests");
    let mut request_id = None;
    let start = std::time::Instant::now();

    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(entries) = fs::read_dir(&requests_dir) {
            let files: Vec<_> = entries.flatten().collect();
            if !files.is_empty() {
                let path = files[0].path();
                let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
                request_id = Some(stem);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let id = request_id.expect("Expected a staged request file to be created");

    println!("TEST DENY: root path is {:?}", root);

    // Write the denied decision file
    let decisions_dir = root
        .join(".heiwa")
        .join("state")
        .join("dispatch")
        .join("approvals")
        .join("decisions");
    fs::create_dir_all(&decisions_dir).unwrap();
    let decision_path = decisions_dir.join(format!("{}.json", id));

    println!("TEST DENY: decision_path is {:?}", decision_path);

    let decision_json = serde_json::json!({
        "id": id,
        "outcome": "denied",
        "decided_at_utc": chrono::Utc::now().to_rfc3339(),
        "operator": "test-decider"
    });
    fs::write(
        &decision_path,
        serde_json::to_string_pretty(&decision_json).unwrap(),
    )
    .unwrap();

    // Await execution receipt by joining the thread
    let (receipts, transcript) = execute_handle.join().unwrap().unwrap();
    assert_eq!(receipts.len(), 1);
    // Should be gated and returned as denied policy error
    assert_eq!(receipts[0].status.as_str(), "denied");
    assert!(receipts[0]
        .error
        .as_ref()
        .unwrap()
        .contains("gated, approval: denied: denied"));
    assert!(transcript[0]
        .output
        .contains("gated, approval: denied: denied"));
}
