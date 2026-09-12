use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn git(root: &Path, args: &[&str]) {
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

fn heiwa(runtime_root: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_heiwa"))
        .env_clear()
        .env("HOME", runtime_root.parent().expect("runtime parent"))
        .env("HEIWA_HOME", runtime_root)
        .env("HEIWA_DISABLE_KEYCHAIN", "1")
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run heiwa")
}

fn successful(output: Output, command: &str) -> Output {
    assert!(
        output.status.success(),
        "{command}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

struct PreparedWork {
    _fixture: tempfile::TempDir,
    runtime_root: PathBuf,
    repo: PathBuf,
    worktree: PathBuf,
    work_id: String,
}

fn prepared_work(intent: &str) -> PreparedWork {
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

/// Write an executable `provider` script that identifies itself on stdout.
fn write_provider(path: &Path, says: &str) {
    std::fs::write(path, format!("#!/bin/sh\nprintf {says}\n")).expect("write provider");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod provider");
    }
}

fn sha256_of(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(std::fs::read(path).expect("read provider"));
    format!("{:x}", hasher.finalize())
}

#[test]
fn relative_provider_runs_the_executable_whose_identity_was_recorded() {
    // `./provider` is canonicalized and hashed against the operator's shell
    // cwd, but the child is spawned after `current_dir` switches to the
    // prepared worktree. With a different `./provider` in each place, the
    // receipt can attest to one binary while another actually ran.
    let fixture = prepared_work("relative provider identity");
    let parent_provider = fixture.repo.join("provider");
    let worktree_provider = fixture.worktree.join("provider");
    write_provider(&parent_provider, "parent");
    write_provider(&worktree_provider, "worktree");

    let run = successful(
        heiwa(
            &fixture.runtime_root,
            &fixture.repo,
            &[
                "work",
                "run",
                &fixture.work_id,
                "--json",
                "--",
                "./provider",
            ],
        ),
        "work run",
    );
    let outcome: serde_json::Value = serde_json::from_slice(&run.stdout).expect("run JSON");

    let ran = outcome["pane_tail"][0].as_str().expect("pane tail");
    let recorded_sha = outcome["executable_sha256"].as_str().expect("sha256");
    let recorded_path = outcome["executable_path"].as_str().expect("path");

    let ran_sha = match ran {
        "parent" => sha256_of(&parent_provider),
        "worktree" => sha256_of(&worktree_provider),
        other => panic!("provider printed {other:?}"),
    };
    assert_eq!(
        recorded_sha, ran_sha,
        "receipt must attest to the program that actually ran \
         (ran={ran}, recorded_path={recorded_path})"
    );
    assert_eq!(
        ran, "parent",
        "`./provider` names the operator's own cwd, which is what was hashed"
    );
}

#[test]
fn work_run_json_stdout_is_exactly_one_json_document_even_when_child_writes_both_streams() {
    let fixture = prepared_work("json output");
    let run = successful(
        heiwa(
            &fixture.runtime_root,
            &fixture.repo,
            &[
                "work",
                "run",
                &fixture.work_id,
                "--json",
                "--",
                "/bin/sh",
                "-c",
                "printf child-out; printf child-err >&2",
            ],
        ),
        "work run",
    );

    let outcome: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout must be one JSON document: {error}: {:?}",
            String::from_utf8_lossy(&run.stdout)
        )
    });
    assert_eq!(outcome["exit_code"], 0);
    let tail = outcome["pane_tail"].as_array().expect("pane tail");
    assert_eq!(tail.len(), 2);
    assert!(tail.contains(&serde_json::json!("child-out")));
    assert!(tail.contains(&serde_json::json!("child-err")));
}

#[test]
fn child_json_flag_after_separator_does_not_change_wrapper_output_mode() {
    let fixture = prepared_work("child flag boundary");
    let run = successful(
        heiwa(
            &fixture.runtime_root,
            &fixture.repo,
            &[
                "work",
                "run",
                &fixture.work_id,
                "--",
                "/bin/sh",
                "-c",
                "printf 'child-json-flag\\n'",
                "child",
                "--json",
            ],
        ),
        "work run with child --json",
    );

    let stdout = String::from_utf8(run.stdout).expect("UTF-8 stdout");
    assert!(
        stdout.starts_with("child-json-flag\n"),
        "human mode must echo child output: {stdout:?}"
    );
    assert!(
        stdout.contains(" ran "),
        "human mode must retain the wrapper summary: {stdout:?}"
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "child flags must not switch the wrapper to JSON mode"
    );
}

/// Every `operator_events.jsonl` under `root`, concatenated.
fn read_journals(root: &Path) -> String {
    fn walk(dir: &Path, out: &mut String) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path
                .file_name()
                .is_some_and(|n| n == "operator_events.jsonl")
            {
                out.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    let mut out = String::new();
    walk(root, &mut out);
    out
}

#[test]
fn a_refused_pane_tail_still_records_that_the_worker_exited() {
    // A provider that echoes a token-shaped string into its output. The
    // evidence sensitivity screen refuses to persist that pane tail — but the
    // process has already exited by then, so dropping the exit event would
    // leave it projected as live forever with no recorded status.
    let fixture = prepared_work("pane rejection");
    let run = heiwa(
        &fixture.runtime_root,
        &fixture.repo,
        &[
            "work",
            "run",
            &fixture.work_id,
            "--json",
            "--",
            "/bin/sh",
            "-c",
            "printf 'sk-DEADBEEFdeadbeef0123456789'",
        ],
    );
    assert!(
        !run.status.success(),
        "the sensitivity screen is expected to refuse this pane tail"
    );

    let journal = read_journals(&fixture.runtime_root);
    assert!(
        journal.contains("worker_launched"),
        "sanity: the launch event should have been recorded"
    );
    assert!(
        !journal.contains("pane_closed"),
        "sanity: the pane tail is the event being refused"
    );
    assert!(
        journal.contains("worker_exited"),
        "a process that already exited must not stay projected as live"
    );
}
