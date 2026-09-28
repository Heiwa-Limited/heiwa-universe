//! Proof must describe the requirements and committed inputs actually checked.

use std::{fs, path::Path, process::Command};

use heiwa_claims::{evaluate, evidence, verify, ClaimState, Registry, VerifyResult};

fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit(root: &Path) {
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "fixture"]);
}

fn repository(verifier: &str, params: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("claims")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    fs::write(root.join("src/input.txt"), "original symbol\n").unwrap();
    fs::write(
        root.join("claims/test.toml"),
        format!(
            r#"
[[claim]]
claim_id = "test.proof"
subject = "src"
claim = "the requested symbol exists"
required_state = "verified"
scope = ["src", "scripts"]
verifier_id = "{verifier}"
{params}
"#
        ),
    )
    .unwrap();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "claims@example.test"]);
    git(root, &["config", "user.name", "Claim Test"]);
    commit(root);
    dir
}

#[test]
fn changing_requirements_invalidates_otherwise_identical_evidence() {
    let dir = repository("symbols-present", "params = { symbols = [\"original\"] }");
    let root = dir.path();
    let registry = Registry::load(root).unwrap();
    assert_eq!(
        verify(root, &registry.claims[0]).unwrap().result,
        VerifyResult::Pass
    );
    assert_eq!(
        evaluate(root, &registry).unwrap()[0].state,
        ClaimState::Verified
    );
    let manifest = root.join("claims/test.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest)
            .unwrap()
            .replace("[\"original\"]", "[\"missing\"]"),
    )
    .unwrap();
    commit(root);
    let changed = Registry::load(root).unwrap();
    assert_eq!(
        evaluate(root, &changed).unwrap()[0].state,
        ClaimState::Degraded
    );
    assert_eq!(
        verify(root, &changed.claims[0]).unwrap().result,
        VerifyResult::Fail
    );
}

#[test]
fn evidence_from_another_claim_cannot_be_relabelled_by_its_filename() {
    let dir = repository("symbols-present", "params = { symbols = [\"original\"] }");
    let root = dir.path();
    let registry = Registry::load(root).unwrap();
    let mut record = verify(root, &registry.claims[0]).unwrap();
    record.claim_id = "another.claim".into();
    fs::write(
        root.join("claims/evidence/test.proof.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    assert_eq!(
        evaluate(root, &registry).unwrap()[0].state,
        ClaimState::Degraded
    );
}

#[test]
fn unsafe_identifiers_cannot_escape_the_evidence_directory() {
    let dir = repository("symbols-present", "params = { symbols = [\"original\"] }");
    let root = dir.path();
    let mut claim = Registry::load(root).unwrap().claims.remove(0);
    for bad in ["../escaped", "/tmp/escaped", ".", "..", "", "x\\y"] {
        claim.claim_id = bad.into();
        assert!(verify(root, &claim).is_err(), "accepted {bad:?}");
    }
    assert!(!root.join("claims/escaped.json").exists());
}

#[test]
fn dirty_manifest_or_unregistered_parameters_cannot_be_attested_at_head() {
    let dir = repository("symbols-present", "params = { symbols = [\"original\"] }");
    let root = dir.path();
    let mut claim = Registry::load(root).unwrap().claims.remove(0);
    claim.params.symbols = vec!["symbol".into()];
    assert!(verify(root, &claim).is_err());
    let original = Registry::load(root).unwrap().claims.remove(0);
    let manifest = root.join("claims/test.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest).unwrap() + "\n# uncommitted\n",
    )
    .unwrap();
    assert!(verify(root, &original).is_err());
}

#[test]
fn a_verifier_cannot_record_a_pass_after_mutating_its_inputs_or_head() {
    for script in [
        "printf changed > src/input.txt\n",
        "git -c core.hooksPath=/dev/null commit --allow-empty -qm changed\n",
        "printf '\n# changed\n' >> claims/test.toml\n",
    ] {
        let dir = repository("l0-acceptance", "");
        let root = dir.path();
        fs::write(root.join("scripts/check_l0_acceptance.sh"), script).unwrap();
        commit(root);
        let claim = Registry::load(root).unwrap().claims.remove(0);
        assert!(
            verify(root, &claim).is_err(),
            "accepted mutating verifier: {script}"
        );
        assert!(evidence::load(root, &claim.claim_id).is_none());
    }
}

#[test]
#[cfg(unix)]
fn text_verification_does_not_follow_tracked_symlinks() {
    let dir = repository(
        "symbols-present",
        "params = { symbols = [\"outside secret\"] }",
    );
    let root = dir.path();
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::write(outside.path(), "outside secret").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("src/link")).unwrap();
    commit(root);
    let claim = Registry::load(root).unwrap().claims.remove(0);
    assert!(verify(root, &claim).is_err());
    assert!(evidence::load(root, &claim.claim_id).is_none());
}
