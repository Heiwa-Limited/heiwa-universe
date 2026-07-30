//! E3 DREX golden eval suite — L1 intent-classification layer.
//!
//! Data-driven goldens over [`parse_turn_intent`] (prompt -> [`Intent`]). Pure
//! and hermetic: no providers, network, STDB, or `heiwa-route`. Fixtures live in
//! `tests/fixtures/drex_golden/l1/*.json`.
//!
//! Layers: L1 here; L2 routing -> `apps/heiwa_core/tests/drex_golden.rs`;
//! L3 CLI smoke -> `apps/heiwa_shell/tests/smoke.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use heiwa_protocol::parse_turn_intent;
use serde::Deserialize;

#[derive(Deserialize)]
struct GoldenCase {
    id: String,
    #[allow(dead_code)]
    description: String,
    #[serde(default)]
    status: Status,
    input: Input,
    expect: Expect,
}

#[derive(Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Status {
    #[default]
    Active,
    /// Documents intended-but-unbuilt behavior (e.g. risk classification).
    Pending,
}

#[derive(Deserialize)]
struct Input {
    prompt: String,
}

#[derive(Deserialize)]
struct Expect {
    #[serde(default)]
    intent: Option<String>,
    /// Risk classification is unbuilt (risk is hardcoded "low" in the route
    /// path). Pending fixtures may record a target risk for documentation; it
    /// is not asserted until a prompt->risk classifier exists.
    #[serde(default)]
    #[allow(dead_code)]
    risk: Option<String>,
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/drex_golden/l1")
}

#[test]
fn drex_golden_l1_classify() {
    let dir = fixtures_dir();
    let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    entries.sort();

    let mut failures: Vec<String> = Vec::new();
    let mut active = 0usize;
    let mut pending = 0usize;

    for path in entries {
        let text = fs::read_to_string(&path).expect("read fixture");
        let case: GoldenCase =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));

        if case.status == Status::Pending {
            pending += 1;
            continue;
        }
        active += 1;

        let turn = parse_turn_intent(&case.input.prompt);
        let actual = turn.intent.as_drex_key();
        if let Some(expected) = &case.expect.intent {
            if actual != expected {
                failures.push(format!(
                    "[{}] intent: expected {expected}, got {actual} (prompt: {:?})",
                    case.id, case.input.prompt
                ));
            }
        }
    }

    println!("drex_golden L1: {active} active, {pending} pending fixture(s)");
    assert!(active > 0, "no active L1 fixtures found in {}", dir.display());
    assert!(
        failures.is_empty(),
        "{} L1 golden failure(s) ({} active, {} pending):\n{}",
        failures.len(),
        active,
        pending,
        failures.join("\n")
    );
}
