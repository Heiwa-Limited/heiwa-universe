//! The System 1 wire contract: question builders and the response decoder.
//!
//! Bodies here are TypeSafe's own documented examples (docs.typesafe.ai/api,
//! read 2026-09-23) wherever one exists, so the decoder is held to the
//! provider's contract rather than to a shape this crate invented.

use heiwa_judgment::decode::decode_response;
use heiwa_judgment::question::{Question, QuestionError, QuestionSet};
use heiwa_judgment::{Answer, ErrorKind};
use serde_json::json;

fn documented_questions() -> QuestionSet {
    QuestionSet::new(vec![
        (
            "frustration".into(),
            Question::score(
                "How frustrated is the customer?",
                ["Calm", "Frustrated", "Very angry"],
            )
            .unwrap(),
        ),
        (
            "department".into(),
            Question::choice(
                "Which team should handle this?",
                [
                    ("billing", "Payments, invoicing, refunds"),
                    ("technical", "Bugs, outages, integrations"),
                    ("sales", "Pricing, upgrades, new accounts"),
                ],
            )
            .unwrap(),
        ),
        (
            "is_urgent".into(),
            Question::noul("Does this convey urgency?").unwrap(),
        ),
    ])
    .unwrap()
}

fn documented_body() -> serde_json::Value {
    json!({
        "model": "jev-1.13.0",
        "answers": {
            "frustration": {
                "type": "score",
                "score": 1.05,
                "legend": { "0": "Calm", "1": "Frustrated", "2": "Very angry" },
                "probabilities": { "0": 0.0, "1": 0.95, "2": 0.05 },
                "confidence": 0.92
            },
            "department": {
                "type": "choice",
                "choice": "billing",
                "probabilities": { "billing": 0.88, "technical": 0.12, "sales": 0.0 },
                "confidence": 0.81
            },
            "is_urgent": { "type": "noul", "noul": 0.95 }
        },
        "usage": { "input_tokens": 304, "output_tokens": 18 }
    })
}

// ---- questions -------------------------------------------------------------

#[test]
fn a_question_set_encodes_to_the_documented_request_shape() {
    let questions = QuestionSet::new(vec![
        (
            "department".into(),
            Question::choice(
                "Which team should handle this?",
                [("billing", "Payments"), ("technical", "Bugs")],
            )
            .unwrap(),
        ),
        (
            "frustration".into(),
            Question::score("How frustrated?", ["Calm", "Angry"]).unwrap(),
        ),
        (
            "is_repeat".into(),
            Question::noul_with_criteria(
                "Has the customer contacted support before?",
                "Mentions a prior attempt",
                "No sign of previous contact",
            )
            .unwrap(),
        ),
    ])
    .unwrap();

    assert_eq!(
        questions.to_wire(),
        json!({
            "department": {
                "type": "choice",
                "instructions": "Which team should handle this?",
                "criteria": { "billing": "Payments", "technical": "Bugs" }
            },
            "frustration": {
                "type": "score",
                "instructions": "How frustrated?",
                "criteria": ["Calm", "Angry"]
            },
            "is_repeat": {
                "type": "noul",
                "instructions": "Has the customer contacted support before?",
                "criteria": {
                    "true": "Mentions a prior attempt",
                    "false": "No sign of previous contact"
                }
            }
        })
    );
}

#[test]
fn a_noul_without_criteria_omits_the_key() {
    assert_eq!(
        Question::noul("Refund requested?").unwrap().to_wire(),
        json!({ "type": "noul", "instructions": "Refund requested?" })
    );
}

#[test]
fn builders_refuse_questions_the_api_would_reject() {
    assert_eq!(
        Question::noul("   ").unwrap_err(),
        QuestionError::EmptyInstructions
    );
    assert_eq!(
        Question::choice("Pick", [("only", "one")]).unwrap_err(),
        QuestionError::TooFewOptions(1)
    );
    assert_eq!(
        Question::choice("Pick", [("a", "x"), ("a", "y")]).unwrap_err(),
        QuestionError::DuplicateOption("a".into())
    );
    assert_eq!(
        Question::score("Rate", ["one"]).unwrap_err(),
        QuestionError::TooFewLevels(1)
    );
    let eleven: Vec<String> = (0..11).map(|i| format!("level {i}")).collect();
    assert_eq!(
        Question::score("Rate", eleven).unwrap_err(),
        QuestionError::TooManyLevels(11)
    );
    let ten: Vec<String> = (0..10).map(|i| format!("level {i}")).collect();
    assert!(Question::score("Rate", ten).is_ok());
}

#[test]
fn a_question_set_needs_unique_ids_and_at_least_one_question() {
    let q = Question::noul("x?").unwrap();
    assert_eq!(
        QuestionSet::new(vec![("a".into(), q.clone()), ("a".into(), q)]).unwrap_err(),
        QuestionError::DuplicateId("a".into())
    );
    assert_eq!(QuestionSet::new(vec![]).unwrap_err(), QuestionError::Empty);
}

#[test]
fn the_digest_changes_when_any_wording_changes() {
    let a = QuestionSet::new(vec![("q".into(), Question::noul("Is it red?").unwrap())]).unwrap();
    let b = QuestionSet::new(vec![("q".into(), Question::noul("Is it red?").unwrap())]).unwrap();
    let c = QuestionSet::new(vec![("q".into(), Question::noul("Is it blue?").unwrap())]).unwrap();
    assert_eq!(a.digest(), b.digest());
    assert_ne!(a.digest(), c.digest());
    assert!(a.digest().starts_with("sha256:"));
}

// ---- decoder ---------------------------------------------------------------

#[test]
fn documented_bodies_decode_into_normalized_answers() {
    let decoded = decode_response(&documented_questions(), &documented_body()).unwrap();

    assert_eq!(decoded.model, "jev-1.13.0");
    assert_eq!(decoded.usage.input_tokens, 304);
    assert_eq!(decoded.usage.output_tokens, 18);
    assert!(decoded.missing.is_empty());
    assert_eq!(
        decoded.answer("frustration"),
        Some(&Answer::Score {
            score: 1.05,
            legend: vec!["Calm".into(), "Frustrated".into(), "Very angry".into()],
            probabilities: vec![0.0, 0.95, 0.05],
            confidence: 0.92,
        })
    );
    // Choice distributions come back in the question's own option order.
    assert_eq!(
        decoded.answer("department"),
        Some(&Answer::Choice {
            choice: "billing".into(),
            probabilities: vec![
                ("billing".into(), 0.88),
                ("technical".into(), 0.12),
                ("sales".into(), 0.0),
            ],
            confidence: 0.81,
        })
    );
    assert_eq!(
        decoded.answer("is_urgent"),
        Some(&Answer::Noul { noul: 0.95 })
    );
}

#[test]
fn a_missing_answer_is_reported_not_fatal() {
    let mut body = documented_body();
    body["answers"].as_object_mut().unwrap().remove("is_urgent");
    let decoded = decode_response(&documented_questions(), &body).unwrap();
    assert_eq!(decoded.missing, vec!["is_urgent".to_string()]);
    assert!(decoded.answer("department").is_some());
}

#[test]
fn an_answer_to_a_question_never_asked_is_ignored() {
    let mut body = documented_body();
    body["answers"]["sentiment"] = json!({ "type": "noul", "noul": 0.2 });
    let decoded = decode_response(&documented_questions(), &body).unwrap();
    assert!(decoded.answer("sentiment").is_none());
}

fn assert_violation(body: serde_json::Value, needle: &str) {
    let error = decode_response(&documented_questions(), &body).unwrap_err();
    assert_eq!(error.kind, ErrorKind::SchemaViolation, "{}", error.message);
    assert!(
        error.message.contains(needle),
        "expected {needle:?} in {:?}",
        error.message
    );
}

#[test]
fn the_decoder_holds_answers_to_the_questions_that_were_asked() {
    // The diagnostic names the question, never the value the provider sent.
    let mut body = documented_body();
    body["answers"]["department"]["choice"] = json!("legal");
    assert_violation(body.clone(), "department");
    let error = decode_response(&documented_questions(), &body).unwrap_err();
    assert!(!error.message.contains("legal"), "{}", error.message);

    let mut body = documented_body();
    body["answers"]["department"]["probabilities"] = json!({ "billing": 0.9, "legal": 0.1 });
    assert_violation(body, "department");

    let mut body = documented_body();
    body["answers"]["frustration"]["probabilities"] = json!({ "0": 0.0, "1": 0.95, "3": 0.05 });
    assert_violation(body, "frustration");

    let mut body = documented_body();
    body["answers"]["frustration"]["legend"] = json!({ "0": "Calm", "1": "Frustrated" });
    assert_violation(body, "frustration");

    let mut body = documented_body();
    body["answers"]["frustration"]["score"] = json!(2.5);
    assert_violation(body, "frustration");

    let mut body = documented_body();
    body["answers"]["is_urgent"] = json!({ "type": "noul", "noul": 1.4 });
    assert_violation(body, "is_urgent");

    let mut body = documented_body();
    body["answers"]["is_urgent"] = json!({ "type": "choice", "noul": 0.9 });
    assert_violation(body, "is_urgent");

    let mut body = documented_body();
    body["answers"]["department"]["confidence"] = json!("high");
    assert_violation(body, "department");
}

#[test]
fn hostile_bodies_are_typed_failures_never_panics() {
    let questions = documented_questions();
    for body in [
        json!(null),
        json!("not json at all"),
        json!(42),
        json!([1, 2, 3]),
        json!({ "model": "jev" }),
        json!({ "answers": "nope" }),
        json!({ "answers": [{ "noul": 0.5 }] }),
        json!({ "answers": {}, "usage": { "input_tokens": -1 } }),
    ] {
        let error = decode_response(&questions, &body).unwrap_err();
        assert_eq!(error.kind, ErrorKind::SchemaViolation, "{body}");
    }
}

#[test]
fn a_body_without_usage_or_model_still_decodes() {
    let body = json!({ "answers": { "is_urgent": { "noul": 0.4 } } });
    let decoded = decode_response(&documented_questions(), &body).unwrap();
    assert_eq!(decoded.model, "unknown");
    assert_eq!(decoded.usage.input_tokens, 0);
    assert_eq!(decoded.missing.len(), 2);
}

// TypeSafe documents every distribution as "floats that sum to 1". A live
// local model returned `score: 2.0` over [1.0, 0.03, 0.85, 0.02, 0.0] — mass
// 1.9 — and an estimate read from it contradicted the model's own score.

#[test]
fn a_distribution_that_does_not_sum_to_one_is_a_schema_violation() {
    let mut body = documented_body();
    body["answers"]["frustration"]["probabilities"] = json!({ "0": 1.0, "1": 0.03, "2": 0.85 });
    assert_violation(body, "frustration");

    let mut body = documented_body();
    body["answers"]["department"]["probabilities"] =
        json!({ "billing": 0.5, "technical": 0.2, "sales": 0.1 });
    assert_violation(body, "department");
}

#[test]
fn a_distribution_rounded_to_two_decimals_still_decodes() {
    // Three options each rounded to two decimals can miss 1 by up to 0.015.
    let mut body = documented_body();
    body["answers"]["department"]["probabilities"] =
        json!({ "billing": 0.33, "technical": 0.33, "sales": 0.33 });
    assert!(decode_response(&documented_questions(), &body).is_ok());
}

// ---- review round 1: provider text is not a diagnostic ----------------------

const SENTINEL: &str = "SYNTHETIC_PRIVATE_PROMPT_SENTINEL";

#[test]
fn decoder_messages_never_repeat_a_provider_supplied_string() {
    let edits: Vec<(&str, Box<dyn Fn(&mut serde_json::Value)>)> = vec![
        (
            "choice",
            Box::new(|b| b["answers"]["department"]["choice"] = json!(SENTINEL)),
        ),
        (
            "distribution key",
            Box::new(|b| {
                b["answers"]["department"]["probabilities"] =
                    json!({ SENTINEL: 0.88, "technical": 0.12, "sales": 0.0 })
            }),
        ),
        (
            "level key",
            Box::new(|b| {
                b["answers"]["frustration"]["probabilities"] =
                    json!({ "0": 0.0, "1": 0.95, SENTINEL: 0.05 })
            }),
        ),
        (
            "legend key",
            Box::new(|b| {
                b["answers"]["frustration"]["legend"] =
                    json!({ "0": "Calm", "1": "Frustrated", SENTINEL: "Very angry" })
            }),
        ),
        (
            "type tag",
            Box::new(|b| b["answers"]["is_urgent"]["type"] = json!(SENTINEL)),
        ),
    ];
    for (case, edit) in edits {
        let mut body = documented_body();
        edit(&mut body);
        let error = decode_response(&documented_questions(), &body).unwrap_err();
        assert_eq!(error.kind, ErrorKind::SchemaViolation, "{case}");
        assert!(
            !error.message.contains(SENTINEL),
            "{case} leaked: {}",
            error.message
        );
    }
}

#[test]
fn a_returned_model_reduces_to_safe_provenance() {
    use heiwa_judgment::decode::{model_provenance, ReturnedModel};

    let same = model_provenance("jev-1.13.0", "jev-1.13.0");
    assert_eq!(same.returned, ReturnedModel::Requested);
    assert!(same.returned_digest.is_none() && same.returned_version.is_none());

    let pinned = model_provenance("jev-latest", "jev-1.13.0");
    assert_eq!(pinned.returned, ReturnedModel::Version);
    assert_eq!(pinned.returned_version.as_deref(), Some("jev-1.13.0"));

    for hostile in [
        format!("{SENTINEL} Bearer sk-synthetic-credential-0000"),
        "jev-1.13.0 plus some prose".to_string(),
        "gpt-4.1".to_string(),
        "jev-".to_string(),
        "jev-1..3".to_string(),
    ] {
        let provenance = model_provenance("jev-latest", &hostile);
        assert_eq!(
            provenance.returned,
            ReturnedModel::Unrecognised,
            "{hostile}"
        );
        assert!(provenance.returned_version.is_none(), "{hostile}");
        // Only what came back is checked: `requested` is our own configuration.
        let returned = serde_json::to_value(&provenance).unwrap();
        let returned = json!({
            "returned": returned["returned"],
            "returned_version": returned["returned_version"],
            "returned_digest": returned["returned_digest"],
        })
        .to_string();
        assert!(
            !returned.contains(&hostile),
            "{hostile} survived: {returned}"
        );
        assert!(returned.contains("sha256:"), "{returned}");
    }
}
