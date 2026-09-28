//! The cross-language gate contract.
//!
//! `packages/heiwa_system1` runs the very same `testdata/gate_cases.json`
//! from TypeScript. Two gates that can disagree about whether an action is
//! safe to auto-dispatch is a serious bug, so neither is allowed to drift:
//! a case added on either side fails on the other until both agree.

use heiwa_judgment::{gate_answer, gate_batch, Answer, Band, Policy, Reason, Thresholds};
use serde_json::Value;

fn fixture() -> Value {
    let raw = include_str!("../testdata/gate_cases.json");
    serde_json::from_str(raw).expect("fixture parses")
}

fn answer_from(v: &Value) -> Answer {
    match v["kind"].as_str().expect("kind") {
        "choice" => Answer::Choice {
            choice: v["choice"].as_str().expect("choice").to_string(),
            probabilities: v["probabilities"]
                .as_object()
                .expect("probabilities")
                .iter()
                .map(|(k, p)| (k.clone(), p.as_f64().expect("probability")))
                .collect(),
            confidence: v["confidence"].as_f64().expect("confidence"),
        },
        "score" => Answer::Score {
            score: v["score"].as_f64().expect("score"),
            legend: v["legend"]
                .as_array()
                .expect("legend")
                .iter()
                .map(|s| s.as_str().unwrap_or_default().to_string())
                .collect(),
            probabilities: v["probabilities"]
                .as_array()
                .expect("probabilities")
                .iter()
                .map(|p| p.as_f64().expect("probability"))
                .collect(),
            confidence: v["confidence"].as_f64().expect("confidence"),
        },
        "noul" => Answer::Noul {
            noul: v["noul"].as_f64().expect("noul"),
        },
        other => panic!("unknown answer kind {other}"),
    }
}

fn band_from(s: &str) -> Band {
    match s {
        "auto" => Band::Auto,
        "deliberate" => Band::Deliberate,
        "quarantine" => Band::Quarantine,
        other => panic!("unknown band {other}"),
    }
}

fn reason_from(s: &str) -> Reason {
    match s {
        "confident" => Reason::Confident,
        "uncertain" => Reason::Uncertain,
        "low_confidence" => Reason::LowConfidence,
        "ambiguous" => Reason::Ambiguous,
        "vetoed" => Reason::Vetoed,
        other => panic!("unknown reason {other}"),
    }
}

/// A predicate cannot be expressed in JSON, so the fixture names the one
/// shape it needs as a threshold and each side builds the closure.
fn veto_noul_above(bar: f64) -> impl Fn(&Answer) -> bool {
    move |answer: &Answer| matches!(answer, Answer::Noul { noul } if *noul > bar)
}

#[test]
fn shipped_defaults_match_the_contract() {
    let f = fixture();
    let defaults = Thresholds::default();
    assert_eq!(defaults.auto, f["defaults"]["auto"].as_f64().unwrap());
    assert_eq!(
        defaults.deliberate,
        f["defaults"]["deliberate"].as_f64().unwrap()
    );
}

#[test]
fn every_single_answer_case_agrees() {
    let f = fixture();
    let cases = f["single"].as_array().expect("single cases");
    assert!(cases.len() >= 15, "contract lost cases: {}", cases.len());

    for case in cases {
        let name = case["name"].as_str().unwrap_or("<unnamed>");
        let answer = answer_from(&case["answer"]);
        let raw = &case["policy"];

        let veto_fn;
        let mut policy = Policy::default();
        if let Some(t) = raw.get("thresholds") {
            policy.thresholds = Some(Thresholds {
                auto: t["auto"].as_f64().expect("auto"),
                deliberate: t["deliberate"].as_f64().expect("deliberate"),
            });
        }
        if let Some(m) = raw.get("min_margin").and_then(|m| m.as_f64()) {
            policy.min_margin = Some(m);
        }
        if let Some(bar) = raw.get("veto_noul_above").and_then(|b| b.as_f64()) {
            veto_fn = veto_noul_above(bar);
            policy.veto = Some(&veto_fn);
        }

        let decision = gate_answer(&answer, &policy);
        let expect = &case["expect"];

        assert_eq!(
            decision.band,
            band_from(expect["band"].as_str().expect("band")),
            "band mismatch: {name}"
        );
        if let Some(r) = expect.get("reason").and_then(|r| r.as_str()) {
            assert_eq!(decision.reason, reason_from(r), "reason mismatch: {name}");
        }
        if let Some(c) = expect.get("confidence").and_then(|c| c.as_f64()) {
            assert!(
                (decision.confidence - c).abs() < 1e-9,
                "confidence mismatch: {name} got {} want {c}",
                decision.confidence
            );
        }
        if let Some(src) = expect.get("confidence_source").and_then(|s| s.as_str()) {
            let got = serde_json::to_value(decision.confidence_source).unwrap();
            assert_eq!(
                got.as_str().unwrap(),
                src,
                "confidence source mismatch: {name}"
            );
        }
    }
}

#[test]
fn every_batch_case_agrees() {
    let f = fixture();
    let cases = f["batch"].as_array().expect("batch cases");
    assert!(cases.len() >= 5, "contract lost cases: {}", cases.len());

    for case in cases {
        let name = case["name"].as_str().unwrap_or("<unnamed>");
        let answers: Vec<(String, Answer)> = case["answers"]
            .as_object()
            .expect("answers")
            .iter()
            .map(|(id, v)| (id.clone(), answer_from(v)))
            .collect();

        // Closures must outlive the policies that borrow them.
        let vetoes: Vec<(String, Box<dyn Fn(&Answer) -> bool>)> = case
            .get("policies")
            .and_then(|p| p.as_object())
            .map(|map| {
                map.iter()
                    .filter_map(|(id, raw)| {
                        raw.get("veto_noul_above")
                            .and_then(|b| b.as_f64())
                            .map(|bar| {
                                let f: Box<dyn Fn(&Answer) -> bool> =
                                    Box::new(veto_noul_above(bar));
                                (id.clone(), f)
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut policies: Vec<(String, Policy<'_>)> = Vec::new();
        if let Some(map) = case.get("policies").and_then(|p| p.as_object()) {
            for (id, raw) in map {
                let mut policy = Policy {
                    informational: raw
                        .get("informational")
                        .and_then(|b| b.as_bool())
                        .unwrap_or(false),
                    ..Policy::default()
                };
                if let Some(v) = vetoes.iter().find(|(key, _)| key == id) {
                    policy.veto = Some(v.1.as_ref());
                }
                policies.push((id.clone(), policy));
            }
        }

        let result = gate_batch(&answers, &policies);
        let expect = &case["expect"];

        assert_eq!(
            result.band,
            band_from(expect["band"].as_str().expect("band")),
            "band mismatch: {name}"
        );

        let mut got = result.blockers.clone();
        got.sort();
        let mut want: Vec<String> = expect["blockers"]
            .as_array()
            .expect("blockers")
            .iter()
            .map(|b| b.as_str().unwrap_or_default().to_string())
            .collect();
        want.sort();
        assert_eq!(got, want, "blockers mismatch: {name}");
    }
}
