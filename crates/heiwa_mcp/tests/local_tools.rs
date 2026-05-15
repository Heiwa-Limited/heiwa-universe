use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use heiwa_mcp::{local_repo_registry, McpError, PolicyDenial};
use heiwa_protocol::{ExecutionScope, RiskClass, ToolLease};
use serde_json::json;

fn test_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("heiwa-mcp-{name}-{nonce}"))
}

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
async fn local_repo_tools_read_list_and_grep_inside_scope() {
    let root = test_root("inside-scope");
    fs::create_dir_all(root.join("apps/heiwa_shell")).expect("mkdir apps");
    fs::write(root.join("README.md"), "DREX routes intent\n").expect("write readme");
    fs::write(root.join("apps/heiwa_shell/Cargo.toml"), "[package]\n").expect("write cargo");

    let registry = local_repo_registry(leased_scope(root.clone()));

    let names = registry.names();
    assert!(names.contains(&"fs.read"));
    assert!(names.contains(&"fs.list"));
    assert!(names.contains(&"repo.grep"));

    let list = registry
        .call("fs.list", json!({ "path": "apps" }))
        .await
        .expect("list apps");
    assert_eq!(list["path"], "apps");
    assert!(list["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .any(|entry| entry["name"] == "heiwa_shell"));

    let read = registry
        .call("fs.read", json!({ "path": "README.md" }))
        .await
        .expect("read readme");
    assert_eq!(read["path"], "README.md");
    assert_eq!(read["content"], "DREX routes intent\n");

    let grep = registry
        .call("repo.grep", json!({ "pattern": "DREX", "path": "." }))
        .await
        .expect("grep repo");
    assert!(grep["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .any(|entry| entry["path"] == "README.md"));

    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn local_repo_tools_fail_closed_without_lease_or_scope() {
    let root = test_root("fail-closed");
    let outside_root = test_root("outside-scope");
    fs::create_dir_all(&root).expect("mkdir root");
    fs::create_dir_all(&outside_root).expect("mkdir outside root");
    fs::write(root.join("README.md"), "secret\n").expect("write readme");
    let outside_file = outside_root.join("outside.txt");
    fs::write(&outside_file, "outside\n").expect("write outside file");

    let unleased = local_repo_registry(ExecutionScope::local_default(root.clone()));
    let denied = unleased
        .call("fs.read", json!({ "path": "README.md" }))
        .await
        .expect_err("missing lease must fail");
    assert!(denied.is_policy_denial(), "expected typed policy denial");
    assert!(matches!(
        denied,
        McpError::PolicyDenied(PolicyDenial::MissingLease { ref tool }) if tool == "fs.read"
    ));

    let leased = local_repo_registry(leased_scope(root.clone()));
    let outside = leased
        .call(
            "fs.read",
            json!({ "path": outside_file.display().to_string() }),
        )
        .await
        .expect_err("outside scope must fail");
    assert!(outside.is_policy_denial(), "expected typed policy denial");
    assert!(matches!(
        outside,
        McpError::PolicyDenied(PolicyDenial::OutsideExecutionScope { .. })
    ));

    fs::remove_dir_all(root).ok();
    fs::remove_dir_all(outside_root).ok();
}
