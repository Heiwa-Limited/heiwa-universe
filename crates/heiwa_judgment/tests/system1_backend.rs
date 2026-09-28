//! System 1 backends against a real local HTTP server: real sockets, real
//! status codes, real timeouts. A stub can only confirm what it was told, so
//! these hold the transport to the documented contract instead.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use heiwa_judgment::backend::{
    Backend, Evaluation, System1Client, FALLBACK_CONFIDENCE_CEILING, TYPESAFE_DEFAULT_MODEL,
};
use heiwa_judgment::question::{Question, QuestionSet};
use heiwa_judgment::{Answer, ErrorKind};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
struct Seen {
    path: String,
    authorization: Option<String>,
    body: Value,
}

type Reply = Arc<dyn Fn(&Seen) -> (u16, String, Duration) + Send + Sync>;

/// A one-route HTTP/1.1 server on an ephemeral port. Every request is
/// recorded, then answered by `reply` after its chosen delay.
struct Server {
    base_url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    location: Arc<Mutex<Option<String>>>,
}

impl Server {
    fn start(reply: Reply) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorded = seen.clone();
        let location: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let redirect_to = location.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let recorded = recorded.clone();
                let reply = reply.clone();
                let redirect_to = redirect_to.clone();
                thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).is_err() {
                        return;
                    }
                    let path = request_line
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("")
                        .to_string();
                    let mut length = 0usize;
                    let mut authorization = None;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty()
                        {
                            break;
                        }
                        let lower = line.to_ascii_lowercase();
                        if let Some(value) = lower.strip_prefix("content-length:") {
                            length = value.trim().parse().unwrap_or(0);
                        }
                        if lower.starts_with("authorization:") {
                            authorization = Some(line["authorization:".len()..].trim().to_string());
                        }
                    }
                    let mut body = vec![0u8; length];
                    let _ = reader.read_exact(&mut body);
                    let seen = Seen {
                        path,
                        authorization,
                        body: serde_json::from_slice(&body).unwrap_or(Value::Null),
                    };
                    recorded.lock().unwrap().push(seen.clone());
                    let (status, payload, delay) = reply(&seen);
                    thread::sleep(delay);
                    let location = redirect_to
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(|to| format!("location: {to}\r\n"))
                        .unwrap_or_default();
                    let response = format!(
                        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\n{location}content-length: {}\r\nconnection: close\r\n\r\n{payload}",
                        payload.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                });
            }
        });
        Server {
            base_url,
            seen,
            location,
        }
    }

    fn with_location(self, location: String) -> Self {
        *self.location.lock().unwrap() = Some(location);
        self
    }

    fn requests(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn reply(status: u16, body: Value) -> Reply {
    let text = body.to_string();
    Arc::new(move |_| (status, text.clone(), Duration::ZERO))
}

fn questions() -> QuestionSet {
    QuestionSet::new(vec![
        (
            "team".into(),
            Question::choice("Which team?", [("billing", "money"), ("technical", "bugs")]).unwrap(),
        ),
        (
            "urgency".into(),
            Question::score("How urgent?", ["low", "mid", "high"]).unwrap(),
        ),
        (
            "refund".into(),
            Question::noul_with_criteria("Refund requested?", "Asks for money back", "Does not")
                .unwrap(),
        ),
    ])
    .unwrap()
}

fn jev_body() -> Value {
    json!({
        "model": "jev-1.13.0",
        "answers": {
            "team": {
                "type": "choice",
                "choice": "billing",
                "probabilities": { "billing": 0.9, "technical": 0.1 },
                "confidence": 0.8
            },
            "urgency": {
                "type": "score",
                "score": 0.2,
                "legend": { "0": "low", "1": "mid", "2": "high" },
                "probabilities": { "0": 0.8, "1": 0.2, "2": 0.0 },
                "confidence": 0.7
            },
            "refund": { "type": "noul", "noul": 0.97 }
        },
        "usage": { "input_tokens": 120, "output_tokens": 9 }
    })
}

/// Every call goes through `System1Client`: the transport policy under test.
async fn evaluate(backend: Backend, state: &str, budget: Duration) -> Evaluation {
    System1Client::new(backend)
        .expect("client")
        .evaluate(state, &questions(), budget)
        .await
}

const BUDGET: Duration = Duration::from_secs(5);

// ---- TypeSafe ----------------------------------------------------------------

#[tokio::test]
async fn typesafe_sends_the_documented_request_and_decodes_the_answer() {
    let server = Server::start(reply(200, jev_body()));
    let backend = Backend::typesafe_at(&server.base_url, "sk-test", TYPESAFE_DEFAULT_MODEL);

    let evaluation = evaluate(backend, "I was charged twice", BUDGET).await;

    assert!(!evaluation.over_budget());
    assert_eq!(evaluation.backend, "typesafe");
    assert_eq!(evaluation.requested_model, "jev-1.13.0");
    let decoded = evaluation.outcome.expect("decoded");
    assert_eq!(decoded.model, "jev-1.13.0");
    assert_eq!(decoded.usage.input_tokens, 120);
    assert_eq!(decoded.answer("refund"), Some(&Answer::Noul { noul: 0.97 }));

    let sent = server.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].path, "/v1/systemone");
    assert_eq!(sent[0].authorization.as_deref(), Some("Bearer sk-test"));
    assert_eq!(sent[0].body["state"], json!("I was charged twice"));
    assert_eq!(sent[0].body["model"], json!("jev-1.13.0"));
    assert_eq!(sent[0].body["questions"], questions().to_wire());
}

#[tokio::test]
async fn typesafe_without_a_key_is_not_configured_and_sends_nothing() {
    let server = Server::start(reply(200, jev_body()));
    let backend = Backend::typesafe_at(&server.base_url, "", TYPESAFE_DEFAULT_MODEL);
    let evaluation = evaluate(backend, "x", BUDGET).await;
    assert_eq!(
        evaluation.outcome.unwrap_err().kind,
        ErrorKind::NotConfigured
    );
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn documented_error_statuses_become_typed_failures() {
    for (status, kind) in [
        (401, ErrorKind::Unauthorized),
        (422, ErrorKind::InvalidRequest),
        (429, ErrorKind::RateLimited),
        (529, ErrorKind::ProviderUnavailable),
    ] {
        let server = Server::start(reply(status, json!({ "error": "nope" })));
        let backend = Backend::typesafe_at(&server.base_url, "sk-test", TYPESAFE_DEFAULT_MODEL);
        let error = evaluate(backend, "x", BUDGET).await.outcome.unwrap_err();
        assert_eq!(error.kind, kind, "status {status}");
        assert_eq!(error.status, Some(status));
    }
}

#[tokio::test]
async fn a_slow_provider_is_a_timeout_at_the_budget_not_a_hang() {
    let body = jev_body().to_string();
    let server = Server::start(Arc::new(move |_| {
        (200, body.clone(), Duration::from_secs(3))
    }));
    let backend = Backend::typesafe_at(&server.base_url, "sk-test", TYPESAFE_DEFAULT_MODEL);
    let evaluation = evaluate(backend, "x", Duration::from_millis(300)).await;
    assert_eq!(evaluation.outcome.unwrap_err().kind, ErrorKind::Timeout);
    assert!(
        evaluation.latency_ms < 2_000,
        "took {}ms",
        evaluation.latency_ms
    );
}

#[tokio::test]
async fn a_200_that_is_not_json_is_a_schema_violation() {
    let server = Server::start(Arc::new(|_| {
        (
            200,
            "<html>captive portal</html>".to_string(),
            Duration::ZERO,
        )
    }));
    let backend = Backend::typesafe_at(&server.base_url, "sk-test", TYPESAFE_DEFAULT_MODEL);
    let error = evaluate(backend, "x", BUDGET).await.outcome.unwrap_err();
    assert_eq!(error.kind, ErrorKind::SchemaViolation);
}

#[tokio::test]
async fn an_unreachable_endpoint_is_a_transport_failure() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let backend = Backend::typesafe_at(
        format!("http://127.0.0.1:{port}"),
        "sk-test",
        TYPESAFE_DEFAULT_MODEL,
    );
    let error = evaluate(backend, "x", BUDGET).await.outcome.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Transport);
}

// ---- structured-output fallback --------------------------------------------

fn completion(content: Value) -> Value {
    json!({
        "choices": [{ "finish_reason": "stop", "message": { "role": "assistant", "content": content.to_string() } }],
        "usage": { "prompt_tokens": 300, "completion_tokens": 40 }
    })
}

/// A small local model's self-reported distributions are not coherent — one
/// live gemma4 answer put 1.9 of probability mass on a five-level scale — so
/// the fallback asks only for what its grammar can enforce: one option or one
/// level (an `enum`, which Ollama honours) and a confidence.
#[tokio::test]
async fn the_fallback_asks_for_a_selection_and_records_a_point_mass() {
    let server = Server::start(reply(
        200,
        completion(json!({
            "team": { "choice": "billing", "confidence": 0.3 },
            "urgency": { "level": 2, "confidence": 1.0 },
            "refund": { "noul": 0.9 }
        })),
    ));
    let backend = Backend::structured_llm(&server.base_url, "gemma4:latest");

    let evaluation = evaluate(backend, "I was charged twice for A-104", BUDGET).await;
    assert_eq!(evaluation.backend, "structured_llm");
    let decoded = evaluation.outcome.expect("decoded");

    // An admitted low confidence is the one number worth believing: kept.
    assert_eq!(
        decoded.answer("team"),
        Some(&Answer::Choice {
            choice: "billing".into(),
            probabilities: vec![("billing".into(), 1.0), ("technical".into(), 0.0)],
            confidence: 0.3,
        })
    );
    // A claimed 1.0 is capped, so a fallback can never reach the auto band.
    assert_eq!(
        decoded.answer("urgency"),
        Some(&Answer::Score {
            score: 2.0,
            legend: vec!["low".into(), "mid".into(), "high".into()],
            probabilities: vec![0.0, 0.0, 1.0],
            confidence: FALLBACK_CONFIDENCE_CEILING,
        })
    );
    assert_eq!(decoded.usage.input_tokens, 300);

    let sent = &server.requests()[0];
    assert_eq!(sent.path, "/v1/chat/completions");
    assert_eq!(sent.body["model"], json!("gemma4:latest"));
    assert!(
        sent.body["max_tokens"].as_u64().unwrap() > 0,
        "an output budget is always sent"
    );
    let schema = &sent.body["response_format"]["json_schema"]["schema"];
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(
        schema["properties"]["team"]["properties"]["choice"]["enum"],
        json!(["billing", "technical"])
    );
    assert_eq!(
        schema["properties"]["urgency"]["properties"]["level"],
        json!({ "type": "integer", "enum": [0, 1, 2] })
    );
    assert!(
        schema["properties"]["urgency"]["properties"]["probabilities"].is_null(),
        "no distribution is asked of a model that cannot produce a coherent one"
    );
    let prompt = sent.body["messages"][1]["content"].as_str().unwrap();
    assert!(prompt.contains("A-104"), "the state reaches the model");
    assert!(prompt.contains("true means: Asks for money back"));
}

#[tokio::test]
async fn the_fallback_does_not_repair_out_of_range_values() {
    for (answers, culprit) in [
        (
            json!({
                "team": { "choice": "billing", "confidence": 0.5 },
                "urgency": { "level": 1, "confidence": 0.5 },
                "refund": { "noul": 2.0 }
            }),
            "refund",
        ),
        (
            json!({
                "team": { "choice": "billing", "confidence": 0.5 },
                "urgency": { "level": 7, "confidence": 0.5 },
                "refund": { "noul": 0.5 }
            }),
            "urgency",
        ),
        (
            json!({
                "team": { "choice": "billing", "confidence": -0.2 },
                "urgency": { "level": 1, "confidence": 0.5 },
                "refund": { "noul": 0.5 }
            }),
            "team",
        ),
    ] {
        let server = Server::start(reply(200, completion(answers)));
        let backend = Backend::structured_llm(&server.base_url, "gemma4:latest");
        let error = evaluate(backend, "x", BUDGET).await.outcome.unwrap_err();
        assert_eq!(error.kind, ErrorKind::SchemaViolation);
        assert!(
            error.message.contains(culprit),
            "{culprit}: {}",
            error.message
        );
    }
}

#[tokio::test]
async fn a_reasoning_model_that_hits_its_budget_is_diagnosed_as_such() {
    let server = Server::start(reply(
        200,
        json!({
            "choices": [{
                "finish_reason": "length",
                "message": { "role": "assistant", "content": "", "reasoning": "Let me think about this carefully..." }
            }]
        }),
    ));
    let backend = Backend::structured_llm(&server.base_url, "qwen3.5:9b");
    let error = evaluate(backend, "x", BUDGET).await.outcome.unwrap_err();
    assert_eq!(error.kind, ErrorKind::SchemaViolation);
    assert!(error.message.contains("reasoning"), "{}", error.message);
}

#[test]
fn only_a_loopback_fallback_counts_as_local() {
    assert!(Backend::typesafe("sk", TYPESAFE_DEFAULT_MODEL).is_remote());
    assert!(!Backend::structured_llm("http://127.0.0.1:11434", "gemma4:latest").is_remote());
    assert!(!Backend::structured_llm("http://localhost:11434", "gemma4:latest").is_remote());
    assert!(Backend::structured_llm("https://api.example.com", "m").is_remote());
    assert!(
        Backend::structured_llm("not a url", "m").is_remote(),
        "unknown is not local"
    );
}

// ---- review round 1 (Astra, 2026-09-23) --------------------------------------
//
// Provider-controlled text is data, not a diagnostic. Every `JudgmentError`
// message is persisted by the shadow journal, so it must be generated here.

const SENTINEL: &str = "SYNTHETIC_PRIVATE_PROMPT_SENTINEL";
const CREDENTIAL: &str = "sk-synthetic-credential-0000";

#[tokio::test]
async fn provider_text_never_enters_an_error_message() {
    let echoed = format!("{{\"detail\": \"{SENTINEL} Authorization: Bearer {CREDENTIAL}\"}}");
    let cases: Vec<(Reply, ErrorKind)> = vec![
        (
            Arc::new(move |_| (422, echoed.clone(), Duration::ZERO)),
            ErrorKind::InvalidRequest,
        ),
        (
            Arc::new(|_| (200, format!("<html>{SENTINEL}</html>"), Duration::ZERO)),
            ErrorKind::SchemaViolation,
        ),
        (
            reply(200, {
                let mut body = jev_body();
                body["answers"]["team"]["choice"] = json!(SENTINEL);
                body
            }),
            ErrorKind::SchemaViolation,
        ),
        (
            reply(200, {
                let mut body = jev_body();
                body["answers"]["team"]["probabilities"] =
                    json!({ SENTINEL: 0.9, "technical": 0.1 });
                body
            }),
            ErrorKind::SchemaViolation,
        ),
        (
            reply(200, {
                let mut body = jev_body();
                body["answers"]["urgency"]["legend"] =
                    json!({ "0": "low", "1": "mid", SENTINEL: "high" });
                body
            }),
            ErrorKind::SchemaViolation,
        ),
        (
            reply(200, {
                let mut body = jev_body();
                body["answers"]["refund"]["type"] = json!(SENTINEL);
                body
            }),
            ErrorKind::SchemaViolation,
        ),
    ];
    for (index, (answer, kind)) in cases.into_iter().enumerate() {
        let server = Server::start(answer);
        let backend = Backend::typesafe_at(&server.base_url, "sk-test", TYPESAFE_DEFAULT_MODEL);
        let error = evaluate(backend, "x", BUDGET).await.outcome.unwrap_err();
        assert_eq!(error.kind, kind, "case {index}: {}", error.message);
        assert!(
            !error.message.contains(SENTINEL),
            "case {index} leaked: {}",
            error.message
        );
        assert!(
            !error.message.contains(CREDENTIAL),
            "case {index} leaked: {}",
            error.message
        );
    }
}

#[tokio::test]
async fn an_endpoint_credential_never_enters_a_transport_error() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let backend = Backend::typesafe_at(
        format!("http://operator:{SENTINEL}@127.0.0.1:{port}"),
        "sk-test",
        TYPESAFE_DEFAULT_MODEL,
    );
    let error = evaluate(backend, "x", BUDGET).await.outcome.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Transport);
    assert!(
        !error.message.contains(SENTINEL),
        "leaked: {}",
        error.message
    );
}

/// An origin that answers every request with a redirect to `location`.
fn redirecting(status: u16, location: String) -> Server {
    Server::start(Arc::new(move |_| (status, String::new(), Duration::ZERO)))
        .with_location(location)
}

#[tokio::test]
async fn a_redirect_never_carries_the_state_to_another_origin() {
    for (status, path) in [(307, "/v1/chat/completions"), (308, "/v1/systemone")] {
        let elsewhere = Server::start(reply(200, jev_body()));
        let origin = redirecting(status, format!("{}{path}", elsewhere.base_url));
        let backend = if path.ends_with("systemone") {
            Backend::typesafe_at(&origin.base_url, "sk-test", TYPESAFE_DEFAULT_MODEL)
        } else {
            Backend::structured_llm(&origin.base_url, "gemma4:latest")
        };
        let evaluation = evaluate(backend, SENTINEL, BUDGET).await;

        assert_eq!(
            origin.requests().len(),
            1,
            "{status}: the configured origin was asked"
        );
        assert!(
            elsewhere.requests().is_empty(),
            "{status}: the redirect target received {:?}",
            elsewhere.requests()
        );
        let error = evaluation.outcome.unwrap_err();
        assert_eq!(
            error.kind,
            ErrorKind::Redirected,
            "{status}: {}",
            error.message
        );
        assert_eq!(error.status, Some(status));
    }
}
