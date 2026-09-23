//! Shadow System 1 judgment for Work-scoped turns, end to end: the real
//! operator runner, a real local HTTP endpoint speaking TypeSafe's documented
//! contract, the real evidence journal, and the report that joins them.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use async_trait::async_trait;
use heiwa_core::drex::{
    CallRisk, CostTruth, ExecutionLocality, ModelCallCandidate, ModelCallRequest, ModelCallStage,
    PrivacyClass, SafetyClass,
};
use heiwa_evidence::OperatorJournal;
use heiwa_judgment::backend::{Backend, TYPESAFE_DEFAULT_MODEL};
use heiwa_protocol::ModelTier;
use heiwa_provider::adapter::{Message, ProviderAdapter, Role, StreamEvent, TokenUsage};
use heiwa_session::operator::{OperatorSessionService, StartTurnRequest};
use heiwa_shell::model_calls::ModelCallExecutor;
use heiwa_shell::operator::{
    OperatorModelTurn, OperatorTurnRunner, OperatorTurnWork, ShadowObserver, ShadowTurn,
};
use heiwa_shell::system1_shadow::{
    render_report, report, turn_route_questions, ShadowJudge, SHADOW_STREAM,
};
use heiwa_work::{work_created_event, WorkId};
use serde_json::{json, Value};
use tokio::sync::mpsc;

// ---- a TypeSafe-shaped endpoint ------------------------------------------

struct Endpoint {
    base_url: String,
    hits: Arc<AtomicUsize>,
}

/// Builds a response body from the request body it received.
type Responder = Arc<dyn Fn(&str) -> String + Send + Sync>;

fn endpoint(status: u16, body: Value) -> Endpoint {
    let payload = body.to_string();
    endpoint_with(status, Arc::new(move |_| payload.clone()), None)
}

/// An origin that answers every request with a redirect to `location`.
fn redirecting(status: u16, location: String) -> Endpoint {
    endpoint_with(status, Arc::new(|_| String::new()), Some(location))
}

fn endpoint_with(status: u16, respond: Responder, location: Option<String>) -> Endpoint {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let location = location
        .map(|to| format!("location: {to}\r\n"))
        .unwrap_or_default();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            counted.fetch_add(1, Ordering::SeqCst);
            let respond = respond.clone();
            let location = location.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                let payload = respond(&String::from_utf8_lossy(&body));
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\n{location}content-length: {}\r\nconnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });
    Endpoint { base_url, hits }
}

/// Jev says: a `build` request needing capability level 2 (class 3).
fn jev_answers() -> Value {
    json!({
        "model": "jev-1.13.0",
        "answers": {
            "intent": {
                "type": "choice",
                "choice": "build",
                "probabilities": {
                    "chat": 0.02, "build": 0.9, "deploy": 0.02, "audit": 0.02,
                    "research": 0.02, "strategy": 0.01, "status_check": 0.01
                },
                "confidence": 0.88
            },
            "capability": {
                "type": "score",
                "score": 2.0,
                "legend": { "0": "a", "1": "b", "2": "c", "3": "d", "4": "e" },
                "probabilities": { "0": 0.02, "1": 0.03, "2": 0.9, "3": 0.04, "4": 0.01 },
                "confidence": 0.9
            }
        },
        "usage": { "input_tokens": 410, "output_tokens": 12 }
    })
}

// ---- turn inputs -----------------------------------------------------------

fn candidate(id: u64, model: &str, class: u8, cost: f64, local: bool) -> ModelCallCandidate {
    ModelCallCandidate {
        tier: ModelTier {
            id,
            model_id: model.into(),
            provider_model_id: model.into(),
            provider: if local { "ollama" } else { "anthropic" }.into(),
            capability_class: class,
            max_context_tokens: 128_000,
            enabled: true,
            last_success_rate: 1.0,
            ..ModelTier::default()
        },
        locality: if local {
            ExecutionLocality::OnDevice
        } else {
            ExecutionLocality::Unverified
        },
        connected: true,
        adapter_capable: true,
        quota_available: true,
        marginal_cost_usd: Some(cost),
        cost_truth: if local {
            CostTruth::LocalZeroCost
        } else {
            CostTruth::ProxyEstimate
        },
    }
}

fn candidates() -> Vec<ModelCallCandidate> {
    vec![
        candidate(1, "small-local", 1, 0.0, true),
        candidate(2, "mid-remote", 3, 0.001, false),
        candidate(3, "frontier-remote", 5, 0.02, false),
    ]
}

fn request(prompt: &str, floor: u8, privacy: PrivacyClass) -> ModelCallRequest {
    ModelCallRequest {
        thread_id: "thread-shadow".into(),
        turn_id: "turn-1".into(),
        work_id: Some("work-shadow".into()),
        call_id: "call-1".into(),
        intent: "build".into(),
        stage: ModelCallStage::Execution,
        raw_text: prompt.into(),
        privacy,
        risk: CallRisk::Low,
        safety: SafetyClass::low_risk_auto_approval(&CallRisk::Low),
        required_capabilities: vec![],
        required_context_tokens: 1,
        minimum_quality_class: floor,
        minimum_success_rate: 0.0,
        maximum_marginal_cost_usd: None,
        preferred_provider: None,
        preferred_model: None,
        allowed_models: vec![],
        excluded_models: vec![],
    }
}

fn shadow_turn(prompt: &str, floor: u8, privacy: PrivacyClass) -> ShadowTurn {
    ShadowTurn {
        thread_id: "thread-shadow".into(),
        turn_id: "turn-1".into(),
        work_id: "work-shadow".into(),
        request: request(prompt, floor, privacy),
        candidates: candidates(),
        remaining_budget_usd: None,
    }
}

fn judge_at(base_url: &str, dir: &Path) -> ShadowJudge {
    ShadowJudge::new(
        Backend::typesafe_at(base_url, "sk-test", TYPESAFE_DEFAULT_MODEL),
        Duration::from_secs(5),
        dir.to_path_buf(),
    )
    .expect("judge")
}

const PROMPT: &str =
    "Split the parser module into lexer and grammar files and keep the tests green";

// ---- the judgment ----------------------------------------------------------

#[test]
fn the_question_set_is_the_documented_turn_route_v1() {
    let questions = turn_route_questions();
    let ids: Vec<&str> = questions.iter().map(|(id, _)| id).collect();
    assert_eq!(ids, ["intent", "capability"]);
    let wire = questions.to_wire();
    assert_eq!(wire["intent"]["type"], json!("choice"));
    assert_eq!(
        wire["intent"]["criteria"].as_object().unwrap().len(),
        7,
        "one option per DREX intent key"
    );
    assert_eq!(wire["capability"]["type"], json!("score"));
    assert_eq!(wire["capability"]["criteria"].as_array().unwrap().len(), 5);
}

#[tokio::test]
async fn a_judged_turn_records_the_recommendation_and_its_exact_counterfactual() {
    let dir = tempfile::tempdir().unwrap();
    let server = endpoint(200, jev_answers());
    let judge = judge_at(&server.base_url, dir.path());

    let record = judge
        .judge(&shadow_turn(PROMPT, 1, PrivacyClass::Standard))
        .await;

    assert_eq!(record["schema"], json!("heiwa.system1_shadow.v1"));
    assert_eq!(record["status"], json!("judged"), "{record:#}");
    assert_eq!(record["work_id"], json!("work-shadow"));
    assert_eq!(record["policy"]["question_set"], json!("turn-route-v1"));
    assert_eq!(record["policy"]["model"], json!("jev-1.13.0"));
    assert_eq!(record["policy"]["floor_rule"], json!("raise_only"));
    assert_eq!(record["policy"]["answer_shape"], json!("distribution"));
    assert_eq!(record["call"]["model"]["requested"], json!("jev-1.13.0"));
    assert_eq!(record["call"]["model"]["returned"], json!("requested"));
    assert_eq!(record["call"]["input_tokens"], json!(410));

    // The deterministic route: floor 1, cheapest admitted model.
    assert_eq!(record["baseline"]["minimum_quality_class"], json!(1));
    assert_eq!(
        record["baseline"]["plan"]["selected"]["model_id"],
        json!("small-local")
    );
    // Jev's route: level 2 of 0..4 is capability class 3.
    assert_eq!(record["recommendation"]["intent"], json!("build"));
    assert_eq!(record["recommendation"]["minimum_quality_class"], json!(3));
    assert_eq!(record["counterfactual"]["minimum_quality_class"], json!(3));
    assert_eq!(
        record["counterfactual"]["plan"]["selected"]["model_id"],
        json!("mid-remote")
    );
    assert_eq!(record["agreement"]["intent"], json!(true));
    assert_eq!(record["agreement"]["floor_delta"], json!(2));
    assert_eq!(record["agreement"]["same_model"], json!(false));
    // Capability gates; intent only informs. 0.9 > 0.85 reaches auto.
    assert_eq!(record["gate"]["band"], json!("auto"));

    // The prompt is identified, never stored.
    assert!(record["input"]["digest"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(record["input"]["chars"], json!(PROMPT.chars().count()));
    assert!(!record.to_string().contains("lexer and grammar"));
}

#[tokio::test]
async fn the_counterfactual_floor_only_ever_raises() {
    let dir = tempfile::tempdir().unwrap();
    let server = endpoint(200, jev_answers());
    let judge = judge_at(&server.base_url, dir.path());

    let record = judge
        .judge(&shadow_turn(PROMPT, 5, PrivacyClass::Standard))
        .await;

    assert_eq!(record["recommendation"]["minimum_quality_class"], json!(3));
    assert_eq!(
        record["counterfactual"]["minimum_quality_class"],
        json!(5),
        "an operator's floor is authority; a judgment may not lower it"
    );
    assert_eq!(record["agreement"]["same_model"], json!(true));
}

#[tokio::test]
async fn a_remote_backend_never_receives_a_local_only_or_sensitive_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let server = endpoint(200, jev_answers());
    let judge = judge_at(&server.base_url, dir.path());

    let local_only = judge
        .judge(&shadow_turn(PROMPT, 1, PrivacyClass::LocalOnly))
        .await;
    assert_eq!(local_only["status"], json!("skipped"));
    assert_eq!(local_only["skip_reason"], json!("privacy_local_only"));

    let sovereign = judge
        .judge(&shadow_turn(PROMPT, 1, PrivacyClass::Sovereign))
        .await;
    assert_eq!(sovereign["skip_reason"], json!("privacy_sovereign"));

    let sensitive = judge
        .judge(&shadow_turn(
            "rotate the key sk-live-1234567890 in the deploy script",
            1,
            PrivacyClass::Standard,
        ))
        .await;
    assert_eq!(sensitive["skip_reason"], json!("sensitive_input"));

    assert_eq!(
        server.hits.load(Ordering::SeqCst),
        0,
        "nothing left the machine"
    );
    // The deterministic baseline is still recorded for every skipped turn.
    assert_eq!(
        sensitive["baseline"]["plan"]["selected"]["model_id"],
        json!("small-local")
    );
}

#[tokio::test]
async fn a_failed_judgment_is_recorded_with_its_kind() {
    let dir = tempfile::tempdir().unwrap();
    let server = endpoint(401, json!({ "error": "invalid key" }));
    let judge = judge_at(&server.base_url, dir.path());

    let record = judge
        .judge(&shadow_turn(PROMPT, 1, PrivacyClass::Standard))
        .await;

    assert_eq!(record["status"], json!("failed"));
    assert_eq!(record["call"]["error"]["kind"], json!("unauthorized"));
    assert_eq!(record["call"]["error"]["status"], json!(401));
    assert!(record["recommendation"].is_null());
    assert_eq!(
        record["baseline"]["plan"]["selected"]["model_id"],
        json!("small-local")
    );
}

#[tokio::test]
async fn observing_a_turn_appends_one_record_to_the_shadow_stream() {
    let dir = tempfile::tempdir().unwrap();
    let server = endpoint(200, jev_answers());
    let judge = judge_at(&server.base_url, dir.path());

    ShadowObserver::observe(&judge, shadow_turn(PROMPT, 1, PrivacyClass::Standard));

    let events = wait_for_records(dir.path(), 1).await;
    assert_eq!(events[0]["turn_id"], json!("turn-1"));
    assert_eq!(events[0]["status"], json!("judged"));
}

async fn wait_for_records(dir: &Path, count: usize) -> Vec<Value> {
    for _ in 0..300 {
        let stream = heiwa_evidence::read_stream(dir, SHADOW_STREAM).unwrap();
        if stream.events.len() >= count {
            return stream
                .events
                .into_iter()
                .map(|event| event.record)
                .collect();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the shadow stream never reached {count} record(s)");
}

// ---- through the real runner and executor, then the report -----------------

/// A provider that answers every call at once.
struct DoneAdapter;

#[async_trait]
impl ProviderAdapter for DoneAdapter {
    async fn send(
        &self,
        _model: &str,
        _messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        stream_tx.send(StreamEvent::Token("done".into())).await?;
        stream_tx
            .send(StreamEvent::Done(TokenUsage::default()))
            .await?;
        Ok(())
    }

    async fn interrupt(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn supported_models(&self) -> Vec<String> {
        vec![]
    }
}

fn model_turn(prompt: &str, candidates: Vec<ModelCallCandidate>) -> OperatorModelTurn {
    OperatorModelTurn {
        request: request(prompt, 1, PrivacyClass::Standard),
        candidates,
        messages: vec![Message {
            role: Role::User,
            content: prompt.into(),
        }],
        remaining_budget_usd: None,
        max_attempts: 1,
        tool_scope: None,
        done_payload: Arc::new(|result| json!({ "text": result.text })),
    }
}

async fn run_to_terminal(
    runner: &OperatorTurnRunner,
    thread: &str,
    request: StartTurnRequest,
    turn: OperatorModelTurn,
) {
    let mut handle = runner
        .submit(thread, request, OperatorTurnWork::Model(Box::new(turn)))
        .expect("submit");
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), handle.recv())
            .await
            .expect("turn timed out")
            .expect("stream closed");
        if frame.is_terminal() {
            return;
        }
    }
}

fn work_request(key: &str) -> StartTurnRequest {
    let mut request = StartTurnRequest::auto(key, PROMPT);
    request.work_id = Some("work-shadow".into());
    request
}

#[tokio::test]
async fn the_report_joins_shadow_records_with_what_the_turns_actually_did() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = Arc::new(OperatorSessionService::new(
        OperatorJournal::new(dir.path().to_path_buf()).unwrap(),
    ));
    sessions.ensure_thread("thread-shadow").unwrap();
    sessions
        .append_event(work_created_event(
            &WorkId::parse("work-shadow").unwrap(),
            "thread-shadow",
            "split the parser",
            "installation-test",
            "2026-09-23T00:00:00Z",
            || "evt-create-work-shadow".to_string(),
        ))
        .unwrap();

    let server = endpoint(200, jev_answers());
    let adapter: Arc<dyn ProviderAdapter> = Arc::new(DoneAdapter);
    let executor = Arc::new(ModelCallExecutor::new(
        Arc::new(move |_: &str, _: &str| Some(adapter.clone())),
        sessions.clone(),
    ));
    let runner = OperatorTurnRunner::new(sessions.clone(), executor)
        .with_shadow_observer(Arc::new(judge_at(&server.base_url, dir.path())));

    // Routed and completed; Jev would have raised the floor to class 3.
    run_to_terminal(
        &runner,
        "thread-shadow",
        work_request("shadow-report-routed"),
        model_turn(PROMPT, candidates()),
    )
    .await;
    // Nothing admissible: DREX plans, finds no route, and the turn is
    // interrupted. Jev's floor cannot change an empty plan.
    let disabled = candidates()
        .into_iter()
        .map(|mut candidate| {
            candidate.tier.enabled = false;
            candidate
        })
        .collect();
    run_to_terminal(
        &runner,
        "thread-shadow",
        work_request("shadow-report-unroutable"),
        model_turn(PROMPT, disabled),
    )
    .await;
    // An unscoped turn is outside the experiment.
    run_to_terminal(
        &runner,
        "default",
        StartTurnRequest::auto("shadow-report-unscoped", "hello"),
        model_turn("hello", candidates()),
    )
    .await;

    wait_for_records(dir.path(), 2).await;
    let report = report(dir.path(), Some("work-shadow")).expect("report");

    assert_eq!(report["schema"], json!("heiwa.system1_shadow.report.v1"));
    assert_eq!(report["work_id"], json!("work-shadow"));
    assert_eq!(report["records"], json!(2), "{report:#}");
    assert_eq!(report["status"]["judged"], json!(2));
    assert_eq!(report["coverage"]["work_model_turns"], json!(2));
    assert_eq!(report["coverage"]["without_record"], json!(0));
    assert_eq!(report["bands"]["auto"], json!(2));
    assert_eq!(report["intent"]["agree"], json!(2));
    assert_eq!(report["route"]["would_raise_floor"], json!(2));
    assert_eq!(report["route"]["would_change_model"], json!(1));

    let changed = &report["outcomes"]["would_change_model"];
    assert_eq!(changed["turns"], json!(1));
    assert_eq!(changed["completed"], json!(1));
    assert_eq!(changed["completed_uncancelled"], json!(1));
    assert_eq!(changed["interrupted"], json!(0));
    assert_eq!(changed["executed_models"]["ollama/small-local"], json!(1));

    let same = &report["outcomes"]["same_route"];
    assert_eq!(same["turns"], json!(1));
    assert_eq!(same["completed"], json!(0));
    assert_eq!(same["interrupted"], json!(1));
    assert_eq!(same["completed_uncancelled"], json!(0));

    assert!(report["caveats"]
        .as_array()
        .unwrap()
        .iter()
        .any(|caveat| caveat.as_str().unwrap().contains("labels")));

    // Unfiltered, the unscoped turn still does not appear: shadow is Work-only.
    let everything = report_all(dir.path());
    assert_eq!(everything["records"], json!(2));
}

fn report_all(dir: &Path) -> Value {
    report(dir, None).expect("report")
}

#[test]
fn a_report_over_an_empty_journal_says_so_rather_than_failing() {
    let dir = tempfile::tempdir().unwrap();
    let report = report(dir.path(), None).expect("report");
    assert_eq!(report["records"], json!(0));
    assert_eq!(report["coverage"]["work_model_turns"], json!(0));
}

// ---- review round 1 (Astra, 2026-09-23) ---------------------------------------
//
// The invariant: nothing a provider sends — response bodies, echoed values,
// invalid keys, its `model` string — is persisted. Checked on the raw JSONL
// bytes the real journal wrote, not on the in-memory record.

const SENTINEL: &str = "SYNTHETIC_PRIVATE_PROMPT_SENTINEL";
const CREDENTIAL: &str = "sk-synthetic-credential-0000";

fn sentinel_turn(privacy: PrivacyClass) -> ShadowTurn {
    shadow_turn(&format!("Summarise {SENTINEL} for the notes"), 1, privacy)
}

/// Judge one turn, persist it through the real journal, and return the
/// persisted record with the raw stream text.
async fn judge_and_persist(judge: &ShadowJudge, turn: &ShadowTurn, dir: &Path) -> (Value, String) {
    let record = judge.judge(turn).await;
    judge.record(&record).expect("record");
    let raw = std::fs::read_to_string(dir.join(format!("{SHADOW_STREAM}.jsonl"))).expect("stream");
    let last = raw.lines().last().expect("one record");
    let persisted = serde_json::from_str::<Value>(last).expect("envelope")["record"].clone();
    (persisted, raw)
}

fn assert_clean(raw: &str, case: &str) {
    assert!(
        !raw.contains(SENTINEL),
        "{case}: provider text reached the journal: {raw}"
    );
    assert!(
        !raw.contains(CREDENTIAL),
        "{case}: a credential reached the journal: {raw}"
    );
}

#[tokio::test]
async fn provider_text_never_reaches_the_shadow_journal() {
    let hostile = format!("{SENTINEL} Authorization: Bearer {CREDENTIAL}");
    let with_answers = |edit: &dyn Fn(&mut Value)| {
        let mut body = jev_answers();
        edit(&mut body);
        body.to_string()
    };
    let cases: Vec<(&str, u16, String, Option<&str>)> = vec![
        // A proxy that echoes the request (and so the prompt) in its error.
        (
            "echoed error body",
            422,
            String::new(),
            Some("invalid_request"),
        ),
        (
            "invalid choice",
            200,
            with_answers(&|body| body["answers"]["intent"]["choice"] = json!(hostile)),
            Some("schema_violation"),
        ),
        (
            "invalid distribution key",
            200,
            with_answers(&|body| {
                body["answers"]["intent"]["probabilities"] = json!({ hostile.clone(): 1.0 })
            }),
            Some("schema_violation"),
        ),
        (
            "echoed model on a valid answer",
            200,
            with_answers(&|body| body["model"] = json!(hostile)),
            None,
        ),
    ];

    for (case, status, body, error_kind) in cases {
        let dir = tempfile::tempdir().unwrap();
        let echo = body.is_empty();
        let respond: Responder = Arc::new(move |request: &str| {
            if echo {
                json!({ "detail": request, "auth": format!("Bearer {CREDENTIAL}") }).to_string()
            } else {
                body.clone()
            }
        });
        let server = endpoint_with(status, respond, None);
        let judge = judge_at(&server.base_url, dir.path());

        let (persisted, raw) =
            judge_and_persist(&judge, &sentinel_turn(PrivacyClass::Standard), dir.path()).await;

        assert_clean(&raw, case);
        match error_kind {
            Some(kind) => {
                assert_eq!(persisted["status"], json!("failed"), "{case}");
                assert_eq!(persisted["call"]["error"]["kind"], json!(kind), "{case}");
                if status != 200 {
                    assert_eq!(
                        persisted["call"]["error"]["status"],
                        json!(status),
                        "{case}"
                    );
                }
            }
            None => {
                assert_eq!(persisted["status"], json!("judged"), "{case}");
                let model = &persisted["call"]["model"];
                assert_eq!(model["requested"], json!("jev-1.13.0"), "{case}");
                assert_eq!(model["returned"], json!("unrecognised"), "{case}");
                assert!(
                    model["returned_digest"]
                        .as_str()
                        .is_some_and(|d| d.starts_with("sha256:")),
                    "{case}: {model}"
                );
            }
        }
    }
}

#[tokio::test]
async fn a_returned_model_is_provenance_not_free_text() {
    let dir = tempfile::tempdir().unwrap();
    let server = endpoint(200, jev_answers());
    let judge = judge_at(&server.base_url, dir.path());
    let (persisted, _) =
        judge_and_persist(&judge, &sentinel_turn(PrivacyClass::Standard), dir.path()).await;
    let model = &persisted["call"]["model"];
    assert_eq!(model["requested"], json!("jev-1.13.0"));
    assert_eq!(model["returned"], json!("requested"), "{model}");
    assert!(model["returned_digest"].is_null());
}

#[tokio::test]
async fn a_local_backend_cannot_be_redirected_to_another_origin() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = endpoint(200, jev_answers());
    let origin = redirecting(307, format!("{}/v1/chat/completions", elsewhere.base_url));
    let judge = ShadowJudge::new(
        Backend::structured_llm(&origin.base_url, "gemma4:latest"),
        Duration::from_secs(5),
        dir.path().to_path_buf(),
    )
    .expect("judge");

    // Local-only is allowed to reach a loopback backend, so the call is made.
    let (persisted, raw) =
        judge_and_persist(&judge, &sentinel_turn(PrivacyClass::LocalOnly), dir.path()).await;

    assert_eq!(
        origin.hits.load(Ordering::SeqCst),
        1,
        "the classified origin was asked"
    );
    assert_eq!(
        elsewhere.hits.load(Ordering::SeqCst),
        0,
        "the redirect target must never receive the local-only state"
    );
    assert_eq!(persisted["status"], json!("failed"));
    assert_eq!(persisted["call"]["error"]["kind"], json!("redirected"));
    assert_eq!(persisted["call"]["error"]["status"], json!(307));
    assert_clean(&raw, "redirect");
}

#[test]
fn a_record_that_still_looks_sensitive_is_withheld() {
    let dir = tempfile::tempdir().unwrap();
    let judge = judge_at("http://127.0.0.1:9", dir.path());
    judge
        .record(&json!({
            "schema": "heiwa.system1_shadow.v1",
            "thread_id": "thread-x",
            "turn_id": "turn-x",
            "work_id": "work-x",
            "status": "judged",
            "call": { "note": format!("Authorization: Bearer {CREDENTIAL}") },
        }))
        .expect("record");

    let raw = std::fs::read_to_string(dir.path().join(format!("{SHADOW_STREAM}.jsonl"))).unwrap();
    assert_clean(&raw, "screen");
    let persisted =
        serde_json::from_str::<Value>(raw.lines().last().unwrap()).unwrap()["record"].clone();
    assert_eq!(persisted["status"], json!("withheld"));
    assert_eq!(
        persisted["turn_id"],
        json!("turn-x"),
        "the turn stays accountable"
    );
}

// ---- review round 1: cost truth survives aggregation ------------------------

/// A provider that reports what the call cost, as an exact provider report.
struct PricedAdapter {
    cost_usd: f64,
}

#[async_trait]
impl ProviderAdapter for PricedAdapter {
    async fn send(
        &self,
        _model: &str,
        _messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        stream_tx.send(StreamEvent::Token("done".into())).await?;
        stream_tx
            .send(StreamEvent::Done(TokenUsage {
                cost_usd: self.cost_usd,
                ..TokenUsage::default()
            }))
            .await?;
        Ok(())
    }

    async fn interrupt(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn supported_models(&self) -> Vec<String> {
        vec![]
    }
}

/// One candidate per kind of cost truth the executor can write. All are
/// class 5, so the counterfactual never moves the route.
fn priced(kind: &str) -> ModelCallCandidate {
    let mut candidate = candidate(0, kind, 5, 0.0, false);
    candidate.tier.provider = kind.to_string();
    match kind {
        // Known zero: a model on this device.
        "local-zero" => {
            candidate.locality = ExecutionLocality::OnDevice;
            candidate.cost_truth = CostTruth::LocalZeroCost;
            candidate.marginal_cost_usd = Some(0.0);
        }
        // Exact: the provider reports the charge (see `PricedAdapter`).
        "paid-exact" => {
            candidate.cost_truth = CostTruth::ProxyEstimate;
            candidate.marginal_cost_usd = Some(0.001);
        }
        "estimated" => {
            candidate.cost_truth = CostTruth::ProxyEstimate;
            candidate.marginal_cost_usd = Some(0.002);
        }
        // Unknown: the executor records `cost_usd: 0.0` with `cannot_confirm`.
        "unknown" => {
            candidate.cost_truth = CostTruth::CannotConfirm;
            candidate.marginal_cost_usd = None;
        }
        other => panic!("no cost kind {other}"),
    }
    candidate
}

#[tokio::test]
async fn the_report_keeps_cost_truth_through_aggregation() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = Arc::new(OperatorSessionService::new(
        OperatorJournal::new(dir.path().to_path_buf()).unwrap(),
    ));
    let works: [(&str, &[&str]); 3] = [
        ("work-known", &["local-zero", "paid-exact"]),
        ("work-estimate", &["local-zero", "estimated"]),
        (
            "work-mixed",
            &["local-zero", "paid-exact", "estimated", "unknown"],
        ),
    ];
    for (work, _) in works {
        let thread = format!("thread-{work}");
        sessions.ensure_thread(&thread).unwrap();
        sessions
            .append_event(work_created_event(
                &WorkId::parse(work).unwrap(),
                &thread,
                "price the turns",
                "installation-test",
                "2026-09-23T00:00:00Z",
                || format!("evt-create-{work}"),
            ))
            .unwrap();
    }

    let server = endpoint(200, jev_answers());
    let exact: Arc<dyn ProviderAdapter> = Arc::new(PricedAdapter { cost_usd: 0.004 });
    let free: Arc<dyn ProviderAdapter> = Arc::new(DoneAdapter);
    let executor = Arc::new(ModelCallExecutor::new(
        Arc::new(move |provider: &str, _: &str| {
            Some(if provider == "paid-exact" {
                exact.clone()
            } else {
                free.clone()
            })
        }),
        sessions.clone(),
    ));
    let runner = OperatorTurnRunner::new(sessions.clone(), executor)
        .with_shadow_observer(Arc::new(judge_at(&server.base_url, dir.path())));

    let mut total = 0;
    for (work, kinds) in works {
        for kind in kinds {
            let mut request = StartTurnRequest::auto(format!("cost-{work}-{kind}"), PROMPT);
            request.work_id = Some(work.to_string());
            run_to_terminal(
                &runner,
                &format!("thread-{work}"),
                request,
                model_turn(PROMPT, vec![priced(kind)]),
            )
            .await;
            total += 1;
        }
    }
    wait_for_records(dir.path(), total).await;

    // All known: one known zero, one exact charge.
    let known = report(dir.path(), Some("work-known")).expect("report");
    let cost = &known["execution_cost"];
    assert_eq!(cost["known_zero"], json!(1), "{known:#}");
    assert_eq!(cost["exact"], json!(1));
    assert_eq!(cost["unknown"], json!(0));
    assert!((cost["known_usd"].as_f64().unwrap() - 0.004).abs() < 1e-12);
    assert!((cost["total_usd"].as_f64().unwrap() - 0.004).abs() < 1e-12);
    assert_eq!(cost["basis"], json!("exact"));
    let per = &known["cost_per_completed_turn"];
    assert!((per["usd"].as_f64().unwrap() - 0.002).abs() < 1e-12);
    assert_eq!(per["basis"], json!("exact"));
    assert_eq!(
        known["outcomes"]["same_route"]["completed_uncancelled"],
        json!(2)
    );
    assert!(known["outcomes"]["same_route"].get("accepted").is_none());

    // An estimate is a total, labelled as one.
    let estimate = report(dir.path(), Some("work-estimate")).expect("report");
    assert_eq!(estimate["execution_cost"]["estimated"], json!(1));
    assert!(
        (estimate["execution_cost"]["estimated_usd"]
            .as_f64()
            .unwrap()
            - 0.002)
            .abs()
            < 1e-12
    );
    assert_eq!(
        estimate["execution_cost"]["basis"],
        json!("includes_estimates")
    );
    assert_eq!(
        estimate["cost_per_completed_turn"]["basis"],
        json!("includes_estimates")
    );

    // One unknown amount means there is no total and no cost per turn —
    // the known and estimated subtotals stay visible, labelled.
    let mixed = report(dir.path(), Some("work-mixed")).expect("report");
    let cost = &mixed["execution_cost"];
    assert_eq!(cost["unknown"], json!(1), "{mixed:#}");
    assert!(cost["total_usd"].is_null(), "an unknown amount is not zero");
    assert_eq!(cost["basis"], json!("incomplete"));
    assert!((cost["known_usd"].as_f64().unwrap() - 0.004).abs() < 1e-12);
    assert!((cost["estimated_usd"].as_f64().unwrap() - 0.002).abs() < 1e-12);
    let per = &mixed["cost_per_completed_turn"];
    assert!(per["usd"].is_null());
    assert_eq!(per["basis"], json!("incomplete"));
    assert_eq!(per["turns_with_unknown_cost"], json!(1));

    let text = render_report(&mixed);
    assert!(text.contains("unknown"), "{text}");
    assert!(text.contains("unavailable"), "{text}");
    // Only the caveats may mention acceptance, and only to deny it.
    let metrics = text.split("caveats:").next().unwrap();
    assert!(
        !metrics.contains("accepted"),
        "completion is not acceptance: {text}"
    );
    let text = render_report(&known);
    assert!(text.contains("exact"), "{text}");
}

// ---- review round 1, addendum: a tool turn is one episode of several calls --
//
// The runner makes a first model call, runs the requested tool, then makes a
// follow-up call with a new call id. Its receipt carries only the follow-up's
// cost. Episode cost comes from every attempt's own route event.

enum LaterStage {
    Charge(f64),
    Fail,
}

/// Asks for a tool on the first call; answers later calls as told. Every call
/// that succeeds reports its exact charge.
struct TwoStageAdapter {
    calls: AtomicUsize,
    first_cost: f64,
    later: LaterStage,
}

#[async_trait]
impl ProviderAdapter for TwoStageAdapter {
    async fn send(
        &self,
        _model: &str,
        _messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let (text, cost) = match (self.calls.fetch_add(1, Ordering::SeqCst), &self.later) {
            (0, _) => (
                r#"{"tool_calls":[{"id":"list-1","name":"fs.list","arguments":{"path":"."}}]}"#
                    .to_string(),
                self.first_cost,
            ),
            (_, LaterStage::Charge(cost)) => ("tool complete".to_string(), *cost),
            (_, LaterStage::Fail) => {
                stream_tx
                    .send(StreamEvent::Error("synthetic provider failure".into()))
                    .await?;
                return Ok(());
            }
        };
        stream_tx.send(StreamEvent::Token(text)).await?;
        stream_tx
            .send(StreamEvent::Done(TokenUsage {
                cost_usd: cost,
                ..TokenUsage::default()
            }))
            .await?;
        Ok(())
    }

    async fn interrupt(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn supported_models(&self) -> Vec<String> {
        vec![]
    }
}

#[tokio::test]
async fn the_report_counts_every_stage_of_a_tool_turn() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let sessions = Arc::new(OperatorSessionService::new(
        OperatorJournal::new(dir.path().to_path_buf()).unwrap(),
    ));
    for work in ["work-two-stage", "work-later-stage-fails"] {
        let thread = format!("thread-{work}");
        sessions.ensure_thread(&thread).unwrap();
        sessions
            .append_event(work_created_event(
                &WorkId::parse(work).unwrap(),
                &thread,
                "use a tool",
                "installation-test",
                "2026-09-23T00:00:00Z",
                || format!("evt-create-{work}"),
            ))
            .unwrap();
    }

    let charged: Arc<dyn ProviderAdapter> = Arc::new(TwoStageAdapter {
        calls: AtomicUsize::new(0),
        first_cost: 0.003,
        later: LaterStage::Charge(0.004),
    });
    let failing: Arc<dyn ProviderAdapter> = Arc::new(TwoStageAdapter {
        calls: AtomicUsize::new(0),
        first_cost: 0.003,
        later: LaterStage::Fail,
    });
    let executor = Arc::new(ModelCallExecutor::new(
        Arc::new(move |provider: &str, _: &str| {
            Some(if provider == "two-stage" {
                charged.clone()
            } else {
                failing.clone()
            })
        }),
        sessions.clone(),
    ));
    let server = endpoint(200, jev_answers());
    let runner = OperatorTurnRunner::new(sessions.clone(), executor)
        .with_shadow_observer(Arc::new(judge_at(&server.base_url, dir.path())));

    for (work, provider, cost_truth, marginal) in [
        (
            "work-two-stage",
            "two-stage",
            CostTruth::ProxyEstimate,
            Some(0.001),
        ),
        // A failed attempt on a candidate with no known price has no known cost.
        (
            "work-later-stage-fails",
            "two-stage-fails",
            CostTruth::CannotConfirm,
            None,
        ),
    ] {
        let mut candidate = candidate(0, provider, 5, 0.0, false);
        candidate.tier.provider = provider.to_string();
        candidate.cost_truth = cost_truth;
        candidate.marginal_cost_usd = marginal;
        let mut turn = model_turn(PROMPT, vec![candidate]);
        turn.tool_scope = Some(heiwa_protocol::ExecutionScope::local_default(
            workspace.path().to_path_buf(),
        ));
        let mut request = StartTurnRequest::auto(format!("two-stage-{work}"), PROMPT);
        request.work_id = Some(work.to_string());
        run_to_terminal(&runner, &format!("thread-{work}"), request, turn).await;
    }
    wait_for_records(dir.path(), 2).await;

    // Both calls were charged exactly; the receipt alone says 0.004.
    let whole = report(dir.path(), Some("work-two-stage")).expect("report");
    let cost = &whole["execution_cost"];
    assert_eq!(
        whole["outcomes"]["same_route"]["completed"],
        json!(1),
        "{whole:#}"
    );
    assert_eq!(cost["exact"], json!(1), "{whole:#}");
    assert!(
        (cost["known_usd"].as_f64().unwrap() - 0.007).abs() < 1e-12,
        "episode cost is every call's charge: {cost}"
    );
    assert!((cost["total_usd"].as_f64().unwrap() - 0.007).abs() < 1e-12);
    assert_eq!(cost["basis"], json!("exact"));

    // The follow-up failed with no known cost: the first call's charge stays
    // in the known subtotal, and there is no total.
    let failed = report(dir.path(), Some("work-later-stage-fails")).expect("report");
    let cost = &failed["execution_cost"];
    assert_eq!(
        failed["outcomes"]["same_route"]["interrupted"],
        json!(1),
        "{failed:#}"
    );
    assert_eq!(cost["unknown"], json!(1), "{failed:#}");
    assert!(
        (cost["known_usd"].as_f64().unwrap() - 0.003).abs() < 1e-12,
        "an earlier charge is not erased by a later failure: {cost}"
    );
    assert!(cost["total_usd"].is_null());
    assert_eq!(cost["basis"], json!("incomplete"));
}
