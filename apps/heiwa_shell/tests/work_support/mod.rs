//! Isolated `heiwa` binary harness for Work CLI integration tests.
//!
//! Every invocation runs the binary Cargo built from this checkout
//! (`CARGO_BIN_EXE_heiwa`, so `CARGO_TARGET_DIR` is honoured) with a cleared
//! environment, a temporary `HOME`/`HEIWA_HOME`, and no Keychain, so a test
//! never reads or writes durable operator state.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The isolated `heiwa` invocation, not yet started.
pub fn heiwa_command(runtime_root: &Path, cwd: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_heiwa"));
    command
        .env_clear()
        .env("HOME", runtime_root.parent().expect("runtime parent"))
        .env("HEIWA_HOME", runtime_root)
        .env("HEIWA_DISABLE_KEYCHAIN", "1")
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C")
        .current_dir(cwd)
        .args(args);
    command
}

pub fn heiwa(runtime_root: &Path, cwd: &Path, args: &[&str]) -> Output {
    heiwa_command(runtime_root, cwd, args)
        .output()
        .expect("run heiwa")
}

pub fn successful(output: Output, command: &str) -> Output {
    assert!(
        output.status.success(),
        "{command}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

pub struct PreparedWork {
    pub _fixture: tempfile::TempDir,
    pub runtime_root: PathBuf,
    pub repo: PathBuf,
    pub worktree: PathBuf,
    pub work_id: String,
}

pub fn prepared_work(intent: &str) -> PreparedWork {
    let fixture = tempfile::tempdir().expect("fixture");
    let runtime_root = fixture.path().join("runtime");
    let repo = fixture.path().join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    git(&repo, &["init", "-q", "-b", "main", "."]);
    git(&repo, &["config", "user.email", "test@heiwa.ltd"]);
    git(&repo, &["config", "user.name", "Heiwa Test"]);
    std::fs::write(repo.join("tracked.txt"), "tracked\n").expect("fixture file");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "fixture"]);

    heiwa_identity::establish_in(
        &runtime_root,
        "Test operator",
        "2026-08-28T00:00:00Z",
        || "install-test".to_string(),
    )
    .expect("identity");

    let created = successful(
        heiwa(&runtime_root, &repo, &["work", "create", intent, "--json"]),
        "work create",
    );
    let created: serde_json::Value = serde_json::from_slice(&created.stdout).expect("create JSON");
    let work_id = created["work_id"].as_str().expect("work id").to_string();

    let prepared = successful(
        heiwa(
            &runtime_root,
            &repo,
            &["workspace", "prepare", &work_id, "--json"],
        ),
        "workspace prepare",
    );
    let prepared: serde_json::Value =
        serde_json::from_slice(&prepared.stdout).expect("prepare JSON");
    let worktree = PathBuf::from(
        prepared["worktree_path"]
            .as_str()
            .expect("prepared worktree path"),
    );

    PreparedWork {
        _fixture: fixture,
        runtime_root,
        repo,
        worktree,
        work_id,
    }
}
