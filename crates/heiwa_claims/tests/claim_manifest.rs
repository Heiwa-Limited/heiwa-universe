//! Manifest loading, against a synthetic repository.
//!
//! Loading is where a hostile or careless manifest gets refused. These tests
//! exist because every one of them is a way a claim could end up in the
//! registry without a real verifier behind it.

use std::fs;
use std::path::{Path, PathBuf};

use heiwa_claims::evidence::{self, Environment, EvidenceRecord, VerifyResult};
use heiwa_claims::manifest::Registry;

fn scaffold(claims_toml: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/demo\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("crates/demo")).unwrap();
    fs::write(
        root.join("crates/demo/Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("claims")).unwrap();
    fs::write(root.join("claims/test.toml"), claims_toml).unwrap();
    dir
}

fn valid_claim(extra: &str) -> String {
    format!(
        r#"
[[claim]]
claim_id = "demo.one"
subject = "crates/demo"
claim = "demo builds"
required_state = "verified"
scope = ["crates/demo", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo"]
verifier_id = "cargo-test"
params = {{ package = "demo" }}
{extra}
"#
    )
}

#[test]
fn a_well_formed_manifest_loads() {
    let dir = scaffold(&valid_claim(""));
    let registry = Registry::load(dir.path()).expect("valid manifest loads");
    assert_eq!(registry.claims.len(), 1);
    assert_eq!(registry.claims[0].claim_id, "demo.one");
}

#[test]
fn an_unknown_verifier_fails_the_whole_load() {
    // Not "skipped with a warning": a registry that drops what it cannot parse
    // reports no failures for exactly the claims nobody checked.
    let dir = scaffold(
        r#"
[[claim]]
claim_id = "demo.one"
subject = "crates/demo"
claim = "demo builds"
required_state = "verified"
scope = ["crates/demo", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo"]
verifier_id = "make-it-so"
"#,
    );
    let err = Registry::load(dir.path()).unwrap_err().to_string();
    assert!(err.contains("make-it-so"), "{err}");
}

#[test]
fn a_scope_that_leaves_the_repository_is_refused() {
    for bad in ["../secrets", "/etc/passwd", "crates/../../elsewhere"] {
        let dir = scaffold(&format!(
            r#"
[[claim]]
claim_id = "demo.one"
subject = "x"
claim = "x"
required_state = "verified"
scope = ["{bad}"]
verifier_id = "cargo-test"
params = {{ package = "demo" }}
"#
        ));
        assert!(
            Registry::load(dir.path()).is_err(),
            "scope `{bad}` should be refused"
        );
    }
}

#[test]
fn an_empty_scope_is_refused() {
    let dir = scaffold(
        r#"
[[claim]]
claim_id = "demo.one"
subject = "x"
claim = "x"
required_state = "verified"
scope = []
verifier_id = "cargo-test"
params = { package = "demo" }
"#,
    );
    assert!(Registry::load(dir.path()).is_err());
}

#[test]
fn duplicate_claim_ids_are_refused() {
    let mut toml = valid_claim("");
    toml.push_str(&valid_claim(""));
    let dir = scaffold(&toml);
    let err = Registry::load(dir.path()).unwrap_err().to_string();
    assert!(err.contains("duplicate"), "{err}");
}

#[test]
fn a_package_outside_the_workspace_is_refused() {
    let dir = scaffold(
        r#"
[[claim]]
claim_id = "demo.one"
subject = "crates/demo"
claim = "demo builds"
required_state = "verified"
scope = ["crates/demo", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo"]
verifier_id = "cargo-test"
params = { package = "not_a_member" }
"#,
    );
    assert!(Registry::load(dir.path()).is_err());
}

#[test]
fn a_manifest_cannot_declare_its_own_state() {
    // `deny_unknown_fields` is the enforcement. If someone adds `state = ...`
    // to a manifest, the load fails rather than the field being ignored — an
    // ignored field would read, to its author, like it worked.
    let dir = scaffold(&valid_claim(r#"state = "verified""#));
    assert!(Registry::load(dir.path()).is_err());
}

#[test]
fn evidence_round_trips_and_unreadable_evidence_reads_as_absent() {
    let dir = scaffold(&valid_claim(""));
    let root: &Path = dir.path();

    let record = EvidenceRecord {
        claim_id: "demo.one".into(),
        claim_digest: "requirements".into(),
        verifier_id: "cargo-test".into(),
        verifier_version: "1".into(),
        result: VerifyResult::Pass,
        commit: "deadbeef".into(),
        scope_digest: "digest".into(),
        verified_at: 42,
        environment: Environment::current(),
        detail: "ok".into(),
    };
    evidence::store(root, &record).expect("store");
    assert_eq!(evidence::load(root, "demo.one").as_ref(), Some(&record));

    fs::write(evidence::path_for(root, "demo.one").unwrap(), "{ not json").unwrap();
    assert!(
        evidence::load(root, "demo.one").is_none(),
        "corrupt evidence must read as absent so the claim reports unproven, \
         not so the registry refuses to run"
    );
}

#[test]
fn oversized_verifier_output_is_truncated_before_it_reaches_git() {
    let detail = EvidenceRecord::truncate_detail("x".repeat(10_000));
    assert!(detail.len() < 2_200, "len was {}", detail.len());
    assert!(detail.ends_with("truncated"));

    let short = EvidenceRecord::truncate_detail("fine".into());
    assert_eq!(short, "fine");
}

#[test]
fn a_scope_covering_the_evidence_store_is_refused() {
    // Recording proof would invalidate the proof. Such a claim can never reach
    // verified, and a permanently degraded claim with no true cause is how a
    // gate earns the reputation that gets it muted.
    for bad in [
        "claims",
        "claims/",
        "claims/evidence",
        "claims/evidence/x.json",
    ] {
        let dir = scaffold(&format!(
            r#"
[[claim]]
claim_id = "demo.one"
subject = "x"
claim = "x"
required_state = "verified"
scope = ["{bad}"]
verifier_id = "cargo-test"
params = {{ package = "demo" }}
"#
        ));
        let err = Registry::load(dir.path())
            .expect_err(&format!("scope `{bad}` should be refused"))
            .to_string();
        assert!(err.contains("converge"), "{err}");
    }
}

#[test]
fn a_scope_naming_a_claim_manifest_is_still_allowed() {
    // Manifests are fair game: editing what a claim says should invalidate its
    // proof. Only the evidence store is off limits.
    let dir = scaffold(
        r#"
[[claim]]
claim_id = "demo.one"
subject = "x"
claim = "x"
required_state = "verified"
scope = ["claims/test.toml", "crates/demo", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo"]
verifier_id = "cargo-test"
params = { package = "demo" }
"#,
    );
    assert!(Registry::load(dir.path()).is_ok());
}

#[test]
fn claim_ids_are_validated_before_becoming_evidence_paths() {
    for bad in ["../escape", "/tmp/escape", "", ".", "..", "shell;fragment"] {
        let dir = scaffold(&valid_claim("").replace("demo.one", bad));
        assert!(Registry::load(dir.path()).is_err(), "accepted {bad:?}");
        assert!(evidence::path_for(dir.path(), bad).is_err());
    }
}

#[test]
fn cargo_claims_cover_local_dependencies_and_build_configuration() {
    let dir = scaffold(&valid_claim(""));
    let root = dir.path();
    fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = [\"crates/demo\", \"crates/helper\"]\n[workspace.dependencies]\nalias = { package = \"helper\", path = \"crates/helper\" }\n").unwrap();
    fs::create_dir_all(root.join("crates/helper")).unwrap();
    fs::write(
        root.join("crates/helper/Cargo.toml"),
        "[package]\nname = \"helper\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let demo = root.join("crates/demo/Cargo.toml");
    fs::write(
        &demo,
        fs::read_to_string(&demo).unwrap() + "\n[dev-dependencies]\nalias = { workspace = true }\n",
    )
    .unwrap();
    assert!(
        Registry::load(root).is_err(),
        "unbound local dev dependency was admitted"
    );
    let complete = valid_claim("").replace("scope = [", "scope = [\"crates/helper\", ");
    fs::write(root.join("claims/test.toml"), complete).unwrap();
    Registry::load(root).expect("complete dependency closure");
}

#[test]
fn cargo_claims_cannot_omit_workspace_build_configuration() {
    for input in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo"] {
        let manifest = valid_claim("").replace(&format!(", \"{input}\""), "");
        let dir = scaffold(&manifest);
        let error = Registry::load(dir.path()).unwrap_err().to_string();
        assert!(error.contains(input), "{error}");
    }
}

fn record(detail: &str) -> EvidenceRecord {
    EvidenceRecord {
        claim_id: "demo.one".into(),
        claim_digest: "requirements".into(),
        verifier_id: "cargo-test".into(),
        verifier_version: "1".into(),
        result: VerifyResult::Pass,
        commit: "deadbeef".into(),
        scope_digest: "digest".into(),
        verified_at: 42,
        environment: Environment::current(),
        detail: detail.into(),
    }
}

/// Every file under `dir` with its bytes. Links are listed, never followed.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if fs::symlink_metadata(&path).unwrap().is_dir() {
                pending.push(path);
            } else {
                files.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    files.sort();
    files
}

#[test]
#[cfg(unix)]
fn linked_evidence_paths_never_redirect_a_write_or_a_read() {
    use std::os::unix::fs::symlink;

    // Each case links one component of `claims/evidence/demo.one.json` into a
    // tree outside the repository that already holds a well-formed record, so
    // following the link would both succeed and be visible.
    for case in ["record", "evidence directory", "claims directory"] {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let outside = tempfile::tempdir().unwrap();
        let outside_evidence = outside.path().join("claims/evidence");
        fs::create_dir_all(&outside_evidence).unwrap();
        fs::write(
            outside_evidence.join("demo.one.json"),
            serde_json::to_string(&record("outside")).unwrap(),
        )
        .unwrap();
        match case {
            "record" => {
                fs::create_dir_all(root.join("claims/evidence")).unwrap();
                symlink(
                    outside_evidence.join("demo.one.json"),
                    root.join("claims/evidence/demo.one.json"),
                )
                .unwrap();
            }
            "evidence directory" => {
                fs::create_dir(root.join("claims")).unwrap();
                symlink(&outside_evidence, root.join("claims/evidence")).unwrap();
            }
            _ => symlink(outside.path().join("claims"), root.join("claims")).unwrap(),
        }
        let before = snapshot(outside.path());

        assert!(
            evidence::store(root, &record("inside")).is_err(),
            "{case}: stored evidence through a link"
        );
        assert_eq!(
            snapshot(outside.path()),
            before,
            "{case}: changed a file outside the repository"
        );
        assert!(
            evidence::load(root, "demo.one").is_none(),
            "{case}: loaded a record from outside the repository"
        );
    }
}

#[test]
fn evidence_parents_must_be_directories_and_records_regular_files() {
    for case in [
        "claims is a file",
        "evidence is a file",
        "record is a directory",
    ] {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        match case {
            "claims is a file" => fs::write(root.join("claims"), "not a directory").unwrap(),
            "evidence is a file" => {
                fs::create_dir(root.join("claims")).unwrap();
                fs::write(root.join("claims/evidence"), "not a directory").unwrap();
            }
            _ => fs::create_dir_all(root.join("claims/evidence/demo.one.json")).unwrap(),
        }
        assert!(
            evidence::store(root, &record("inside")).is_err(),
            "{case}: store accepted it"
        );
        assert!(
            evidence::load(root, "demo.one").is_none(),
            "{case}: load accepted it"
        );
    }
}

#[test]
fn concurrent_stores_leave_one_complete_record_and_no_temporaries() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().to_path_buf();
    let writers: Vec<_> = (0..8)
        .map(|writer| {
            let root = root.clone();
            std::thread::spawn(move || {
                let written = record(&format!("writer {writer}"));
                evidence::store(&root, &written).map(|_| written)
            })
        })
        .collect();
    let written: Vec<EvidenceRecord> = writers
        .into_iter()
        .map(|writer| writer.join().unwrap().expect("concurrent store"))
        .collect();

    let stored = evidence::load(&root, "demo.one").expect("one complete record");
    assert!(written.contains(&stored), "{stored:?}");
    let names: Vec<String> = fs::read_dir(root.join("claims/evidence"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, vec!["demo.one.json"]);
}
