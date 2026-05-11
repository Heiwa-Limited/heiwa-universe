use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use heiwa_protocol::{ExecutionScope, ToolLease};
use heiwa_shell::agentic::{run_agentic_turn_with_responses, AgenticTurnInput};

fn test_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("heiwa-shell-agentic-{nonce}"))
}

fn leased_scope(root: PathBuf) -> ExecutionScope {
    let mut scope = ExecutionScope::local_default(root);
    for name in ["fs.read", "fs.list", "repo.grep"] {
        scope.tool_leases.push(ToolLease {
            name: name.to_string(),
            risk_class: "host_safe_readonly".to_string(),
            allowed: true,
        });
    }
    scope
}

#[tokio::test]
async fn agentic_smoke_dispatches_fs_list_and_records_receipt() {
    let root = test_root();
    fs::create_dir_all(root.join("apps/heiwa_shell")).expect("mkdir heiwa_shell");
    fs::create_dir_all(root.join("apps/heiwa_app")).expect("mkdir heiwa_app");

    let output = run_agentic_turn_with_responses(AgenticTurnInput {
        prompt: "list this repo's apps/".to_string(),
        scope: leased_scope(root.clone()),
        model_responses: vec![
            r#"{"tool_calls":[{"name":"fs.list","arguments":{"path":"apps"}}]}"#.to_string(),
            "apps/ contains heiwa_app and heiwa_shell".to_string(),
        ],
    })
    .await
    .expect("agentic turn");

    assert!(output.final_answer.contains("heiwa_shell"));
    assert_eq!(output.tool_receipts.len(), 1);
    assert_eq!(output.tool_receipts[0].tool_name, "fs.list");
    assert_eq!(output.tool_receipts[0].status.as_str(), "success");
    assert_eq!(output.tool_receipts[0].provider, "test");
    assert!(
        output.tool_transcript[0].output.contains("heiwa_shell"),
        "tool output should include listed app"
    );

    fs::remove_dir_all(root).ok();
}
