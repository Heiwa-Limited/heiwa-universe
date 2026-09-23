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
    let mut body = documented_body();
    body["answers"]["department"]["choice"] = json!("legal");
    assert_violation(body, "legal");

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
